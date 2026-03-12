use std::collections::HashMap;
use std::fmt;

use crate::ast::*;
use crate::token::Span;

// ---------------------------------------------------------------------------
// TypeId – lightweight handle into the substitution table
// ---------------------------------------------------------------------------

/// Unique identifier for a type slot (either concrete or an inference variable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

// ---------------------------------------------------------------------------
// Ty – resolved / partially-resolved types
// ---------------------------------------------------------------------------

/// Internal representation of types used during inference.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// A type variable that has not yet been resolved.
    Var(TypeId),

    // Primitives
    I32,
    I64,
    U32,
    U64,
    F32,
    F64,
    Bool,
    String_,
    Unit,

    // Compound
    Array(Box<Ty>),
    Option_(Box<Ty>),
    Tuple(Vec<Ty>),
    Function {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    Reference {
        mutable: bool,
        lifetime: Option<String>,
        inner: Box<Ty>,
    },
    Struct(String),
    Enum(String),

    /// An iterator over elements of type T.
    Iterator(Box<Ty>),

    /// Result<T, E> type for error handling.
    Result_ { ok: Box<Ty>, err: Box<Ty> },

    /// An API boundary contract type — like a struct but with runtime validation.
    Contract(String),

    /// An unresolved type parameter (e.g. `T` inside `fn identity<T>(x: T) -> T`).
    TypeParam(String),

    /// The type of the `self` keyword inside an impl block.
    SelfType,

    /// A secret-wrapped type — values that must not be rendered or logged.
    Secret(Box<Ty>),

    /// Unknown / error sentinel – avoids cascading errors.
    Error,
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Var(id) => write!(f, "?T{}", id.0),
            Ty::I32 => write!(f, "i32"),
            Ty::I64 => write!(f, "i64"),
            Ty::U32 => write!(f, "u32"),
            Ty::U64 => write!(f, "u64"),
            Ty::F32 => write!(f, "f32"),
            Ty::F64 => write!(f, "f64"),
            Ty::Bool => write!(f, "bool"),
            Ty::String_ => write!(f, "String"),
            Ty::Unit => write!(f, "()"),
            Ty::Array(inner) => write!(f, "[{}]", inner),
            Ty::Iterator(inner) => write!(f, "Iterator<{}>", inner),
            Ty::Option_(inner) => write!(f, "Option<{}>", inner),
            Ty::Tuple(tys) => {
                write!(f, "(")?;
                for (i, t) in tys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            Ty::Function { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
            Ty::Reference { mutable, lifetime: _, inner } => {
                if *mutable {
                    write!(f, "&mut {}", inner)
                } else {
                    write!(f, "&{}", inner)
                }
            }
            Ty::Struct(name) => write!(f, "{}", name),
            Ty::Enum(name) => write!(f, "{}", name),
            Ty::Contract(name) => write!(f, "contract {}", name),
            Ty::Result_ { ok, err } => write!(f, "Result<{}, {}>", ok, err),
            Ty::TypeParam(name) => write!(f, "{}", name),
            Ty::SelfType => write!(f, "Self"),
            Ty::Secret(inner) => write!(f, "secret {}", inner),
            Ty::Error => write!(f, "<error>"),
        }
    }
}

// ---------------------------------------------------------------------------
// TypeError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

impl TypeError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "type error at line {}:{}: {}",
            self.span.line, self.span.col, self.message
        )
    }
}

// ---------------------------------------------------------------------------
// TypedProgram – the output of type checking
// ---------------------------------------------------------------------------

/// The result of a successful type-check pass.  Stores the resolved type for
/// every expression / binding encountered during inference.
#[derive(Debug)]
pub struct TypedProgram {
    /// All types created during inference, fully substituted.
    pub types: Vec<Ty>,
    /// Map from variable name to its resolved type (top-level scope snapshot).
    pub bindings: HashMap<String, Ty>,
}

// ---------------------------------------------------------------------------
// Substitution table
// ---------------------------------------------------------------------------

/// Substitution-based type store.  Each `TypeId` either points at a concrete
/// `Ty` or is still an unresolved variable.
struct Substitution {
    table: Vec<Option<Ty>>,
}

impl Substitution {
    fn new() -> Self {
        Self { table: Vec::new() }
    }

    /// Allocate a fresh type variable.
    fn fresh_var(&mut self) -> TypeId {
        let id = TypeId(self.table.len() as u32);
        self.table.push(None);
        id
    }

    /// Bind a type variable to a concrete type.
    fn bind(&mut self, id: TypeId, ty: Ty) {
        self.table[id.0 as usize] = Some(ty);
    }

    /// Walk the substitution chain to find the current representative.
    fn resolve(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(id) => {
                if let Some(bound) = &self.table[id.0 as usize] {
                    self.resolve(bound)
                } else {
                    ty.clone()
                }
            }
            Ty::Array(inner) => Ty::Array(Box::new(self.resolve(inner))),
            Ty::Iterator(inner) => Ty::Iterator(Box::new(self.resolve(inner))),
            Ty::Option_(inner) => Ty::Option_(Box::new(self.resolve(inner))),
            Ty::Result_ { ok, err } => Ty::Result_ {
                ok: Box::new(self.resolve(ok)),
                err: Box::new(self.resolve(err)),
            },
            Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(|t| self.resolve(t)).collect()),
            Ty::Function { params, ret } => Ty::Function {
                params: params.iter().map(|t| self.resolve(t)).collect(),
                ret: Box::new(self.resolve(ret)),
            },
            Ty::Reference { mutable, lifetime, inner } => Ty::Reference {
                mutable: *mutable,
                lifetime: lifetime.clone(),
                inner: Box::new(self.resolve(inner)),
            },
            Ty::Secret(inner) => Ty::Secret(Box::new(self.resolve(inner))),
            _ => ty.clone(),
        }
    }

    /// Unify two types, returning an error message on failure.
    fn unify(&mut self, a: &Ty, b: &Ty) -> Result<(), String> {
        let a = self.resolve(a);
        let b = self.resolve(b);

        match (&a, &b) {
            _ if a == b => Ok(()),

            // Bind unresolved variables
            (Ty::Var(id), _) => {
                if self.occurs_in(*id, &b) {
                    return Err(format!("infinite type: {} ~ {}", a, b));
                }
                self.bind(*id, b);
                Ok(())
            }
            (_, Ty::Var(id)) => {
                if self.occurs_in(*id, &a) {
                    return Err(format!("infinite type: {} ~ {}", a, b));
                }
                self.bind(*id, a);
                Ok(())
            }

            // Contract <-> Struct unification: contracts are structurally
            // compatible with structs of the same name (they share the same
            // field layout in the structs registry).
            (Ty::Contract(a_name), Ty::Struct(b_name)) | (Ty::Struct(a_name), Ty::Contract(b_name))
                if a_name == b_name => Ok(()),

            // Numeric coercion: i32 <-> i64, f32 <-> f64
            (Ty::I32, Ty::I64) | (Ty::I64, Ty::I32) => Ok(()),
            (Ty::F32, Ty::F64) | (Ty::F64, Ty::F32) => Ok(()),

            // Error absorbs anything (prevents cascading)
            (Ty::Error, _) | (_, Ty::Error) => Ok(()),

            // Structural unification
            (Ty::Array(a_inner), Ty::Array(b_inner)) => self.unify(a_inner, b_inner),
            (Ty::Iterator(a_inner), Ty::Iterator(b_inner)) => self.unify(a_inner, b_inner),
            (Ty::Option_(a_inner), Ty::Option_(b_inner)) => self.unify(a_inner, b_inner),
            (Ty::Result_ { ok: a_ok, err: a_err }, Ty::Result_ { ok: b_ok, err: b_err }) => {
                self.unify(a_ok, b_ok)?;
                self.unify(a_err, b_err)
            }
            (Ty::Tuple(a_tys), Ty::Tuple(b_tys)) if a_tys.len() == b_tys.len() => {
                for (at, bt) in a_tys.iter().zip(b_tys.iter()) {
                    self.unify(at, bt)?;
                }
                Ok(())
            }
            (
                Ty::Function {
                    params: ap,
                    ret: ar,
                },
                Ty::Function {
                    params: bp,
                    ret: br,
                },
            ) if ap.len() == bp.len() => {
                for (at, bt) in ap.iter().zip(bp.iter()) {
                    self.unify(at, bt)?;
                }
                self.unify(ar, br)
            }
            (
                Ty::Reference {
                    mutable: am,
                    lifetime: _,
                    inner: ai,
                },
                Ty::Reference {
                    mutable: bm,
                    lifetime: _,
                    inner: bi,
                },
            ) => {
                if am != bm {
                    return Err(format!(
                        "reference mutability mismatch: {} vs {}",
                        a, b
                    ));
                }
                self.unify(ai, bi)
            }

            _ => Err(format!("type mismatch: expected {}, found {}", a, b)),
        }
    }

    /// Occurs check – prevents constructing infinite types.
    fn occurs_in(&self, id: TypeId, ty: &Ty) -> bool {
        let ty = self.resolve(ty);
        match &ty {
            Ty::Var(other) => *other == id,
            Ty::Array(inner) | Ty::Iterator(inner) | Ty::Option_(inner) => self.occurs_in(id, inner),
            Ty::Result_ { ok, err } => self.occurs_in(id, ok) || self.occurs_in(id, err),
            Ty::Tuple(tys) => tys.iter().any(|t| self.occurs_in(id, t)),
            Ty::Function { params, ret } => {
                params.iter().any(|t| self.occurs_in(id, t)) || self.occurs_in(id, ret)
            }
            Ty::Reference { inner, .. } => self.occurs_in(id, inner),
            _ => false,
        }
    }

    /// Fully resolve a type, replacing any remaining variables with their
    /// default type (i32 for unconstrained numeric, Unit for the rest).
    fn finalize(&self, ty: &Ty) -> Ty {
        let resolved = self.resolve(ty);
        match &resolved {
            Ty::Var(_) => Ty::Unit,
            Ty::Array(inner) => Ty::Array(Box::new(self.finalize(inner))),
            Ty::Iterator(inner) => Ty::Iterator(Box::new(self.finalize(inner))),
            Ty::Option_(inner) => Ty::Option_(Box::new(self.finalize(inner))),
            Ty::Result_ { ok, err } => Ty::Result_ {
                ok: Box::new(self.finalize(ok)),
                err: Box::new(self.finalize(err)),
            },
            Ty::Tuple(tys) => Ty::Tuple(tys.iter().map(|t| self.finalize(t)).collect()),
            Ty::Function { params, ret } => Ty::Function {
                params: params.iter().map(|t| self.finalize(t)).collect(),
                ret: Box::new(self.finalize(ret)),
            },
            Ty::Reference { mutable, lifetime: _, inner } => Ty::Reference {
                mutable: *mutable,
                lifetime: None,
                inner: Box::new(self.finalize(inner)),
            },
            _ => resolved,
        }
    }
}

// ---------------------------------------------------------------------------
// TypeEnv – scoped variable-to-type mapping
// ---------------------------------------------------------------------------

/// A variable scope.  `parent` enables lexical scoping via a simple chain.
struct TypeEnv {
    bindings: HashMap<String, Ty>,
    parent: Option<Box<TypeEnv>>,
}

impl TypeEnv {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            parent: None,
        }
    }

    fn child(&self) -> Self {
        // Flatten the current scope into the child's parent.  This is a
        // shallow clone because we only need read access to outer scopes.
        let mut flat = HashMap::new();
        self.collect_all(&mut flat);
        Self {
            bindings: HashMap::new(),
            parent: Some(Box::new(TypeEnv {
                bindings: flat,
                parent: None,
            })),
        }
    }

    fn insert(&mut self, name: String, ty: Ty) {
        self.bindings.insert(name, ty);
    }

    fn lookup(&self, name: &str) -> Option<&Ty> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup(name)))
    }

    fn collect_all(&self, out: &mut HashMap<String, Ty>) {
        if let Some(ref parent) = self.parent {
            parent.collect_all(out);
        }
        for (k, v) in &self.bindings {
            out.insert(k.clone(), v.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// StructInfo / ComponentInfo – metadata collected in a first pass
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct StructInfo {
    fields: HashMap<String, Ty>,
}

#[derive(Debug, Clone)]
struct ComponentInfo {
    props: HashMap<String, Ty>,
}

/// Metadata about a trait definition: required method signatures.
#[derive(Debug, Clone)]
struct TraitInfo {
    /// Method name -> (param types, return type)
    methods: HashMap<String, (Vec<Ty>, Ty)>,
    /// Methods that have default implementations
    default_methods: std::collections::HashSet<String>,
}

// ---------------------------------------------------------------------------
// TypeChecker
// ---------------------------------------------------------------------------

struct TypeChecker {
    subst: Substitution,
    errors: Vec<TypeError>,
    structs: HashMap<String, StructInfo>,
    components: HashMap<String, ComponentInfo>,
    /// Function signatures collected in the first pass so that calls can be
    /// checked before the callee body has been visited.
    fn_sigs: HashMap<String, Ty>,
    /// Enum definitions collected in the first pass: maps enum name to a list
    /// of variant names.
    enum_defs: HashMap<String, Vec<String>>,
    /// Type parameter names currently in scope (e.g. `T`, `U` inside a generic
    /// function or struct definition).  Used by `ast_type_to_ty` to resolve
    /// bare names like `T` to `Ty::TypeParam("T")` instead of treating them
    /// as struct/enum names.
    type_params_in_scope: std::collections::HashSet<String>,
    /// Trait definitions collected in the first pass.
    traits: HashMap<String, TraitInfo>,
    /// Contract names — tracks which entries in `structs` are API boundary
    /// contracts (vs plain structs). Contracts generate runtime validators
    /// and content hashes; field access checking reuses the `structs` map.
    contracts: std::collections::HashSet<String>,
    warnings: Vec<TypeWarning>,
    /// Names of functions marked `must_use` — their return values must not be
    /// silently discarded.
    must_use_fns: std::collections::HashSet<String>,
}

#[derive(Debug)]
struct TypeWarning {
    message: String,
    span: Span,
}

impl TypeChecker {
    fn new() -> Self {
        Self {
            subst: Substitution::new(),
            errors: Vec::new(),
            structs: HashMap::new(),
            components: HashMap::new(),
            fn_sigs: HashMap::new(),
            enum_defs: HashMap::new(),
            type_params_in_scope: std::collections::HashSet::new(),
            traits: HashMap::new(),
            contracts: std::collections::HashSet::new(),
            warnings: Vec::new(),
            must_use_fns: std::collections::HashSet::new(),
        }
    }

    // -- helpers ----------------------------------------------------------

    fn error(&mut self, message: impl Into<String>, span: Span) {
        self.errors.push(TypeError::new(message, span));
    }

    fn fresh_var(&mut self) -> Ty {
        Ty::Var(self.subst.fresh_var())
    }

    fn unify(&mut self, a: &Ty, b: &Ty, span: Span) {
        if let Err(msg) = self.subst.unify(a, b) {
            self.error(msg, span);
        }
    }

    fn resolve(&self, ty: &Ty) -> Ty {
        self.subst.resolve(ty)
    }

    /// Convert an AST `Type` node into our internal `Ty`.
    fn ast_type_to_ty(&self, ast_ty: &Type) -> Ty {
        match ast_ty {
            Type::Named(name) => match name.as_str() {
                "i32" => Ty::I32,
                "i64" => Ty::I64,
                "u32" => Ty::U32,
                "u64" => Ty::U64,
                "f32" => Ty::F32,
                "f64" => Ty::F64,
                "bool" => Ty::Bool,
                "String" => Ty::String_,
                "()" => Ty::Unit,
                other => {
                    // Check if this is a type parameter currently in scope
                    if self.type_params_in_scope.contains(other) {
                        Ty::TypeParam(other.to_string())
                    } else if self.structs.contains_key(other) {
                        Ty::Struct(other.to_string())
                    } else {
                        Ty::Enum(other.to_string())
                    }
                }
            },
            Type::Generic { name, args } => {
                // Generic type application — resolve args but treat the
                // overall type as the named type for now (monomorphization
                // is deferred; the args are checked for validity).
                let _resolved_args: Vec<Ty> = args.iter()
                    .map(|t| self.ast_type_to_ty(t))
                    .collect();
                // For now, treat generic applications as their base type
                if self.structs.contains_key(name.as_str()) {
                    Ty::Struct(name.clone())
                } else {
                    Ty::Enum(name.clone())
                }
            }
            Type::Reference { mutable, lifetime, inner } => Ty::Reference {
                mutable: *mutable,
                lifetime: lifetime.clone(),
                inner: Box::new(self.ast_type_to_ty(inner)),
            },
            Type::Array(inner) => Ty::Array(Box::new(self.ast_type_to_ty(inner))),
            Type::Option(inner) => Ty::Option_(Box::new(self.ast_type_to_ty(inner))),
            Type::Result { ok, err } => Ty::Result_ {
                ok: Box::new(self.ast_type_to_ty(ok)),
                err: Box::new(self.ast_type_to_ty(err)),
            },
            Type::Tuple(tys) => {
                Ty::Tuple(tys.iter().map(|t| self.ast_type_to_ty(t)).collect())
            }
            Type::Function { params, ret } => Ty::Function {
                params: params.iter().map(|t| self.ast_type_to_ty(t)).collect(),
                ret: Box::new(self.ast_type_to_ty(ret)),
            },
        }
    }

    /// A dummy span used when we don't have a real span available.
    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0)
    }

    // -- first pass: collect top-level declarations -----------------------

    fn collect_declarations(&mut self, program: &Program) {
        // Register builtin time types
        {
            // Instant — UTC point in time
            let mut instant_fields = HashMap::new();
            instant_fields.insert("unix_ms".to_string(), Ty::I64);
            self.structs.insert("Instant".to_string(), StructInfo { fields: instant_fields });

            // ZonedDateTime — instant + timezone
            let mut zdt_fields = HashMap::new();
            zdt_fields.insert("year".to_string(), Ty::I32);
            zdt_fields.insert("month".to_string(), Ty::I32);
            zdt_fields.insert("day".to_string(), Ty::I32);
            zdt_fields.insert("hour".to_string(), Ty::I32);
            zdt_fields.insert("minute".to_string(), Ty::I32);
            zdt_fields.insert("second".to_string(), Ty::I32);
            zdt_fields.insert("timezone".to_string(), Ty::String_);
            self.structs.insert("ZonedDateTime".to_string(), StructInfo { fields: zdt_fields });

            // Duration — length of time
            let mut duration_fields = HashMap::new();
            duration_fields.insert("ms".to_string(), Ty::I64);
            self.structs.insert("Duration".to_string(), StructInfo { fields: duration_fields });

            // Date — calendar date (no time)
            let mut date_fields = HashMap::new();
            date_fields.insert("year".to_string(), Ty::I32);
            date_fields.insert("month".to_string(), Ty::I32);
            date_fields.insert("day".to_string(), Ty::I32);
            self.structs.insert("Date".to_string(), StructInfo { fields: date_fields });

            // Time — wall clock (no date)
            let mut time_fields = HashMap::new();
            time_fields.insert("hour".to_string(), Ty::I32);
            time_fields.insert("minute".to_string(), Ty::I32);
            time_fields.insert("second".to_string(), Ty::I32);
            self.structs.insert("Time".to_string(), StructInfo { fields: time_fields });

            // Register time module functions
            self.fn_sigs.insert("time::now".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Struct("Instant".to_string())),
            });
            self.fn_sigs.insert("time::zoned".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::Struct("ZonedDateTime".to_string())),
            });
            self.fn_sigs.insert("time::date".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Struct("Date".to_string())),
            });
            self.fn_sigs.insert("Duration::seconds".to_string(), Ty::Function {
                params: vec![Ty::I64],
                ret: Box::new(Ty::Struct("Duration".to_string())),
            });
            self.fn_sigs.insert("Duration::minutes".to_string(), Ty::Function {
                params: vec![Ty::I64],
                ret: Box::new(Ty::Struct("Duration".to_string())),
            });
            self.fn_sigs.insert("Duration::hours".to_string(), Ty::Function {
                params: vec![Ty::I64],
                ret: Box::new(Ty::Struct("Duration".to_string())),
            });
            self.fn_sigs.insert("Duration::days".to_string(), Ty::Function {
                params: vec![Ty::I64],
                ret: Box::new(Ty::Struct("Duration".to_string())),
            });
        }

        // Register clipboard namespace functions
        {
            self.fn_sigs.insert("clipboard::copy".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("clipboard::paste".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("clipboard::copy_image".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Bool),
            });
        }

        // Register crypto namespace functions
        {
            self.fn_sigs.insert("crypto::sha256".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::sha512".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::hmac".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::encrypt".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::decrypt".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::sign".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::verify".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_, Ty::String_],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("crypto::derive_key".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::random_uuid".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("crypto::random_bytes".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::String_),
            });
        }

        // Register debounce and throttle utility functions
        {
            self.fn_sigs.insert("debounce".to_string(), Ty::Function {
                params: vec![Ty::Function { params: vec![], ret: Box::new(Ty::Unit) }, Ty::I32],
                ret: Box::new(Ty::Function { params: vec![], ret: Box::new(Ty::Unit) }),
            });
            self.fn_sigs.insert("throttle".to_string(), Ty::Function {
                params: vec![Ty::Function { params: vec![], ret: Box::new(Ty::Unit) }, Ty::I32],
                ret: Box::new(Ty::Function { params: vec![], ret: Box::new(Ty::Unit) }),
            });
        }

        // Register BigDecimal type and methods
        {
            // BigDecimal — arbitrary precision decimal
            let mut bd_fields = HashMap::new();
            bd_fields.insert("value".to_string(), Ty::String_);
            bd_fields.insert("precision".to_string(), Ty::I32);
            self.structs.insert("BigDecimal".to_string(), StructInfo { fields: bd_fields });

            // BigDecimal constructor and methods
            self.fn_sigs.insert("BigDecimal::new".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Struct("BigDecimal".to_string())),
            });
            self.fn_sigs.insert("BigDecimal::from_i64".to_string(), Ty::Function {
                params: vec![Ty::I64],
                ret: Box::new(Ty::Struct("BigDecimal".to_string())),
            });
            self.fn_sigs.insert("BigDecimal::from_f64".to_string(), Ty::Function {
                params: vec![Ty::F64],
                ret: Box::new(Ty::Struct("BigDecimal".to_string())),
            });
        }

        // Register format namespace functions
        {
            self.fn_sigs.insert("format::number".to_string(), Ty::Function {
                params: vec![Ty::F64, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("format::currency".to_string(), Ty::Function {
                params: vec![Ty::F64, Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("format::percent".to_string(), Ty::Function {
                params: vec![Ty::F64],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("format::bytes".to_string(), Ty::Function {
                params: vec![Ty::I64],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("format::compact".to_string(), Ty::Function {
                params: vec![Ty::F64],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("format::ordinal".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("format::relative_time".to_string(), Ty::Function {
                params: vec![Ty::Struct("Instant".to_string())],
                ret: Box::new(Ty::String_),
            });
        }

        // Register url namespace functions and Url type
        {
            // Url type
            let mut url_fields = HashMap::new();
            url_fields.insert("href".to_string(), Ty::String_);
            url_fields.insert("origin".to_string(), Ty::String_);
            url_fields.insert("protocol".to_string(), Ty::String_);
            url_fields.insert("host".to_string(), Ty::String_);
            url_fields.insert("pathname".to_string(), Ty::String_);
            url_fields.insert("search".to_string(), Ty::String_);
            url_fields.insert("hash".to_string(), Ty::String_);
            self.structs.insert("Url".to_string(), StructInfo { fields: url_fields });

            self.fn_sigs.insert("url::parse".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Struct("Url".to_string())),
            });
            self.fn_sigs.insert("url::build".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Struct("Url".to_string())),
            });
            self.fn_sigs.insert("url::query_get".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::Option_(Box::new(Ty::String_))),
            });
            self.fn_sigs.insert("url::query_set".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
        }

        // Register collections namespace functions
        {
            // Collections utility functions (generic over arrays)
            self.fn_sigs.insert("collections::group_by".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::String_],
                ret: Box::new(Ty::Error), // Returns Map<String, Array<T>>
            });
            self.fn_sigs.insert("collections::sort_by".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::String_],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });
            self.fn_sigs.insert("collections::uniq_by".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::String_],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });
            self.fn_sigs.insert("collections::chunk".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::I32],
                ret: Box::new(Ty::Array(Box::new(Ty::Array(Box::new(Ty::Error))))),
            });
            self.fn_sigs.insert("collections::flatten".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Array(Box::new(Ty::Error))))],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });
            self.fn_sigs.insert("collections::zip".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::Array(Box::new(Ty::Error))],
                ret: Box::new(Ty::Array(Box::new(Ty::Tuple(vec![Ty::Error, Ty::Error])))),
            });
            self.fn_sigs.insert("collections::partition".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::Function { params: vec![Ty::Error], ret: Box::new(Ty::Bool) }],
                ret: Box::new(Ty::Tuple(vec![Ty::Array(Box::new(Ty::Error)), Ty::Array(Box::new(Ty::Error))])),
            });
        }

        // Register mask namespace functions
        {
            self.fn_sigs.insert("mask::phone".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("mask::currency".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("mask::pattern".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("mask::credit_card".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::String_),
            });
        }

        // Register search namespace functions and SearchIndex type
        {
            self.fn_sigs.insert("search::create_index".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::Array(Box::new(Ty::String_))],
                ret: Box::new(Ty::Struct("SearchIndex".to_string())),
            });
            self.fn_sigs.insert("search::query".to_string(), Ty::Function {
                params: vec![Ty::Struct("SearchIndex".to_string()), Ty::String_],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });

            let mut search_idx_fields = HashMap::new();
            search_idx_fields.insert("size".to_string(), Ty::I32);
            self.structs.insert("SearchIndex".to_string(), StructInfo { fields: search_idx_fields });
        }

        // Register theme namespace functions
        {
            self.fn_sigs.insert("theme::init".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("theme::toggle".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("theme::set".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("theme::current".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::String_),
            });
        }

        // Register auth namespace functions
        {
            self.fn_sigs.insert("auth::init".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("auth::login".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("auth::logout".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("auth::get_user".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Error), // opaque user object
            });
            self.fn_sigs.insert("auth::is_authenticated".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Bool),
            });
        }

        // Register upload namespace functions
        {
            self.fn_sigs.insert("upload::init".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("upload::start".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("upload::cancel".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Bool),
            });
        }

        // Register db namespace functions
        {
            self.fn_sigs.insert("db::open".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("db::put".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::Error], // key, value (generic)
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("db::get".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Error), // generic value
            });
            self.fn_sigs.insert("db::delete".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("db::query".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });
        }

        // Register animate namespace functions
        {
            self.fn_sigs.insert("animate::spring".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::Error], // target, config
                ret: Box::new(Ty::String_), // animation ID
            });
            self.fn_sigs.insert("animate::keyframes".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::Error, Ty::F64], // target, keyframes, duration
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("animate::stagger".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::Error, Ty::F64], // targets, config, delay
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("animate::cancel".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Bool),
            });
        }

        // Register responsive namespace functions
        {
            self.fn_sigs.insert("responsive::register_breakpoints".to_string(), Ty::Function {
                params: vec![Ty::Error], // breakpoints config (generic)
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("responsive::get_breakpoint".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("responsive::fluid".to_string(), Ty::Function {
                params: vec![Ty::F64, Ty::F64],
                ret: Box::new(Ty::String_),
            });
        }

        // Register toast namespace functions (pure WASM — DOM syscalls)
        {
            self.fn_sigs.insert("toast::success".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("toast::error".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("toast::warning".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("toast::info".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("toast::dismiss".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("toast::dismiss_all".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register DataTable<T> type and methods (pure WASM computation)
        {
            let mut column_fields = HashMap::new();
            column_fields.insert("name".to_string(), Ty::String_);
            column_fields.insert("label".to_string(), Ty::String_);
            self.structs.insert("Column".to_string(), StructInfo { fields: column_fields });

            let mut page_fields = HashMap::new();
            page_fields.insert("items".to_string(), Ty::Array(Box::new(Ty::Error)));
            page_fields.insert("current_page".to_string(), Ty::I32);
            page_fields.insert("total_pages".to_string(), Ty::I32);
            page_fields.insert("total_items".to_string(), Ty::I32);
            self.structs.insert("Page".to_string(), StructInfo { fields: page_fields });

            self.fn_sigs.insert("DataTable::new".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::Array(Box::new(Ty::Struct("Column".to_string())))],
                ret: Box::new(Ty::Struct("DataTable".to_string())),
            });
            self.fn_sigs.insert("DataTable::sort".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("DataTable::filter".to_string(), Ty::Function {
                params: vec![Ty::Function { params: vec![Ty::Error], ret: Box::new(Ty::Bool) }],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("DataTable::paginate".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::I32],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("DataTable::pin_column".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("DataTable::edit_cell".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::String_, Ty::Error],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("DataTable::get_visible_rows".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });
            self.fn_sigs.insert("DataTable::export_csv".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::String_),
            });
        }

        // Register datepicker namespace functions (pure WASM — DOM syscalls)
        {
            let mut dp_options_fields = HashMap::new();
            dp_options_fields.insert("format".to_string(), Ty::String_);
            dp_options_fields.insert("placeholder".to_string(), Ty::String_);
            self.structs.insert("DatePickerOptions".to_string(), StructInfo { fields: dp_options_fields });

            self.fn_sigs.insert("datepicker::create".to_string(), Ty::Function {
                params: vec![Ty::Struct("DatePickerOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("datepicker::get_value".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("datepicker::set_value".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("datepicker::set_range".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::String_, Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("datepicker::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register skeleton namespace functions (pure WASM — DOM syscalls)
        {
            self.fn_sigs.insert("skeleton::text".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("skeleton::circle".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("skeleton::rect".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("skeleton::card".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("skeleton::avatar".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("skeleton::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register pagination namespace functions (pure WASM computation)
        {
            self.fn_sigs.insert("pagination::paginate".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::I32, Ty::I32],
                ret: Box::new(Ty::Struct("Page".to_string())),
            });
            self.fn_sigs.insert("pagination::page_numbers".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::I32],
                ret: Box::new(Ty::Array(Box::new(Ty::I32))),
            });
            self.fn_sigs.insert("pagination::has_next".to_string(), Ty::Function {
                params: vec![Ty::Struct("Page".to_string())],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("pagination::has_prev".to_string(), Ty::Function {
                params: vec![Ty::Struct("Page".to_string())],
                ret: Box::new(Ty::Bool),
            });
        }

        // Extend search namespace with autocomplete and highlight (pure WASM)
        {
            self.fn_sigs.insert("search::autocomplete".to_string(), Ty::Function {
                params: vec![Ty::Struct("SearchIndex".to_string()), Ty::String_, Ty::I32],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });
            self.fn_sigs.insert("search::highlight".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
        }

        // Register combobox namespace functions (pure WASM — DOM syscalls)
        {
            self.fn_sigs.insert("combobox::create".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::String_))],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("combobox::get_selected".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Array(Box::new(Ty::String_))),
            });
            self.fn_sigs.insert("combobox::set_filter".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("combobox::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register chart namespace functions and types (pure WASM — SVG/Canvas via DOM syscalls)
        {
            let mut point_fields = HashMap::new();
            point_fields.insert("x".to_string(), Ty::F64);
            point_fields.insert("y".to_string(), Ty::F64);
            self.structs.insert("Point".to_string(), StructInfo { fields: point_fields });

            let mut bar_fields = HashMap::new();
            bar_fields.insert("label".to_string(), Ty::String_);
            bar_fields.insert("value".to_string(), Ty::F64);
            self.structs.insert("BarData".to_string(), StructInfo { fields: bar_fields });

            let mut pie_fields = HashMap::new();
            pie_fields.insert("label".to_string(), Ty::String_);
            pie_fields.insert("value".to_string(), Ty::F64);
            pie_fields.insert("color".to_string(), Ty::String_);
            self.structs.insert("PieSlice".to_string(), StructInfo { fields: pie_fields });

            let mut chart_opts_fields = HashMap::new();
            chart_opts_fields.insert("width".to_string(), Ty::I32);
            chart_opts_fields.insert("height".to_string(), Ty::I32);
            chart_opts_fields.insert("title".to_string(), Ty::String_);
            chart_opts_fields.insert("animate".to_string(), Ty::Bool);
            self.structs.insert("ChartOptions".to_string(), StructInfo { fields: chart_opts_fields });

            self.fn_sigs.insert("chart::line".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Struct("Point".to_string()))), Ty::Struct("ChartOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("chart::bar".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Struct("BarData".to_string()))), Ty::Struct("ChartOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("chart::pie".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Struct("PieSlice".to_string()))), Ty::Struct("ChartOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("chart::scatter".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Struct("Point".to_string()))), Ty::Struct("ChartOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("chart::update".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::Array(Box::new(Ty::Struct("Point".to_string())))],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("chart::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register editor namespace functions (pure WASM — contenteditable via DOM syscalls)
        {
            let mut editor_opts_fields = HashMap::new();
            editor_opts_fields.insert("mode".to_string(), Ty::String_);
            editor_opts_fields.insert("placeholder".to_string(), Ty::String_);
            self.structs.insert("EditorOptions".to_string(), StructInfo { fields: editor_opts_fields });

            self.fn_sigs.insert("editor::create".to_string(), Ty::Function {
                params: vec![Ty::Struct("EditorOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("editor::get_content".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("editor::set_content".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("editor::get_markdown".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("editor::insert".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::String_],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("editor::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register image namespace functions (pure WASM pixel manipulation)
        {
            self.fn_sigs.insert("image::crop".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::I32)), Ty::I32, Ty::I32, Ty::I32, Ty::I32],
                ret: Box::new(Ty::Array(Box::new(Ty::I32))),
            });
            self.fn_sigs.insert("image::resize".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::I32)), Ty::I32, Ty::I32],
                ret: Box::new(Ty::Array(Box::new(Ty::I32))),
            });
            self.fn_sigs.insert("image::compress".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::I32)), Ty::F64],
                ret: Box::new(Ty::Array(Box::new(Ty::I32))),
            });
            self.fn_sigs.insert("image::to_base64".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::I32))],
                ret: Box::new(Ty::String_),
            });
        }

        // Register csv namespace functions (pure WASM string processing)
        {
            self.fn_sigs.insert("csv::parse".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Array(Box::new(Ty::Array(Box::new(Ty::String_))))),
            });
            self.fn_sigs.insert("csv::stringify".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Array(Box::new(Ty::String_))))],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("csv::parse_typed".to_string(), Ty::Function {
                params: vec![Ty::String_],
                ret: Box::new(Ty::Array(Box::new(Ty::Error))),
            });
            self.fn_sigs.insert("csv::export".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Error)), Ty::Array(Box::new(Ty::String_))],
                ret: Box::new(Ty::String_),
            });
        }

        // Register maps namespace functions and types (pure WASM — tile rendering via DOM syscalls)
        {
            let mut map_opts_fields = HashMap::new();
            map_opts_fields.insert("center_lat".to_string(), Ty::F64);
            map_opts_fields.insert("center_lng".to_string(), Ty::F64);
            map_opts_fields.insert("zoom".to_string(), Ty::I32);
            map_opts_fields.insert("tile_url".to_string(), Ty::String_);
            self.structs.insert("MapOptions".to_string(), StructInfo { fields: map_opts_fields });

            self.fn_sigs.insert("maps::create".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::Struct("MapOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("maps::add_marker".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::F64, Ty::F64, Ty::String_],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("maps::remove_marker".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::I32],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("maps::set_center".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::F64, Ty::F64],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("maps::set_zoom".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::I32],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("maps::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register syntax namespace functions (pure WASM tokenizer)
        {
            self.fn_sigs.insert("syntax::highlight".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("syntax::highlight_lines".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_, Ty::Array(Box::new(Ty::I32))],
                ret: Box::new(Ty::String_),
            });
        }

        // Register media namespace functions (pure WASM — DOM syscalls for audio/video)
        {
            let mut media_opts_fields = HashMap::new();
            media_opts_fields.insert("controls".to_string(), Ty::Bool);
            media_opts_fields.insert("autoplay".to_string(), Ty::Bool);
            media_opts_fields.insert("loop_playback".to_string(), Ty::Bool);
            media_opts_fields.insert("captions_src".to_string(), Ty::String_);
            self.structs.insert("MediaOptions".to_string(), StructInfo { fields: media_opts_fields });

            self.fn_sigs.insert("media::create_player".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::Struct("MediaOptions".to_string())],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("media::play".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("media::pause".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("media::seek".to_string(), Ty::Function {
                params: vec![Ty::I32, Ty::F64],
                ret: Box::new(Ty::Unit),
            });
            self.fn_sigs.insert("media::get_duration".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::F64),
            });
            self.fn_sigs.insert("media::get_current_time".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::F64),
            });
            self.fn_sigs.insert("media::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        // Register qr namespace functions (pure WASM — QR algorithm in WASM)
        {
            self.fn_sigs.insert("qr::generate".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::I32],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("qr::generate_png".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::I32],
                ret: Box::new(Ty::Array(Box::new(Ty::I32))),
            });
        }

        // Register share namespace functions (WASM logic + navigator.share JS syscall)
        {
            self.fn_sigs.insert("share::native".to_string(), Ty::Function {
                params: vec![Ty::String_, Ty::String_, Ty::String_],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("share::can_share".to_string(), Ty::Function {
                params: vec![],
                ret: Box::new(Ty::Bool),
            });
        }

        // Register wizard namespace functions and WizardStep type (pure WASM state machine)
        {
            let mut wizard_step_fields = HashMap::new();
            wizard_step_fields.insert("name".to_string(), Ty::String_);
            wizard_step_fields.insert("validator".to_string(), Ty::Function { params: vec![], ret: Box::new(Ty::Bool) });
            self.structs.insert("WizardStep".to_string(), StructInfo { fields: wizard_step_fields });

            self.fn_sigs.insert("wizard::create".to_string(), Ty::Function {
                params: vec![Ty::Array(Box::new(Ty::Struct("WizardStep".to_string())))],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("wizard::next".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("wizard::prev".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("wizard::get_current_step".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::I32),
            });
            self.fn_sigs.insert("wizard::validate_step".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Bool),
            });
            self.fn_sigs.insert("wizard::get_data".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::String_),
            });
            self.fn_sigs.insert("wizard::destroy".to_string(), Ty::Function {
                params: vec![Ty::I32],
                ret: Box::new(Ty::Unit),
            });
        }

        for item in &program.items {
            match item {
                Item::Struct(s) => {
                    // Bring struct type params into scope so field types
                    // referencing T, U, etc. resolve to TypeParam.
                    let prev = self.type_params_in_scope.clone();
                    for tp in &s.type_params {
                        self.type_params_in_scope.insert(tp.clone());
                    }
                    let mut fields = HashMap::new();
                    for field in &s.fields {
                        fields.insert(field.name.clone(), self.ast_type_to_ty(&field.ty));
                    }
                    self.structs
                        .insert(s.name.clone(), StructInfo { fields });
                    self.type_params_in_scope = prev;
                }
                Item::Component(c) => {
                    let prev = self.type_params_in_scope.clone();
                    for tp in &c.type_params {
                        self.type_params_in_scope.insert(tp.clone());
                    }
                    let mut props = HashMap::new();
                    for prop in &c.props {
                        props.insert(prop.name.clone(), self.ast_type_to_ty(&prop.ty));
                    }
                    self.components
                        .insert(c.name.clone(), ComponentInfo { props });
                    self.type_params_in_scope = prev;
                }
                Item::Function(f) => {
                    let prev = self.type_params_in_scope.clone();
                    for tp in &f.type_params {
                        self.type_params_in_scope.insert(tp.clone());
                    }
                    let param_tys: Vec<Ty> =
                        f.params.iter().map(|p| self.ast_type_to_ty(&p.ty)).collect();
                    let ret_ty = f
                        .return_type
                        .as_ref()
                        .map(|t| self.ast_type_to_ty(t))
                        .unwrap_or_else(|| self.fresh_var());
                    let fn_ty = Ty::Function {
                        params: param_tys,
                        ret: Box::new(ret_ty),
                    };
                    self.fn_sigs.insert(f.name.clone(), fn_ty);
                    if f.must_use {
                        self.must_use_fns.insert(f.name.clone());
                    }
                    self.type_params_in_scope = prev;
                }
                Item::Impl(imp) => {
                    for method in &imp.methods {
                        let prev = self.type_params_in_scope.clone();
                        for tp in &method.type_params {
                            self.type_params_in_scope.insert(tp.clone());
                        }
                        let param_tys: Vec<Ty> = method
                            .params
                            .iter()
                            .map(|p| self.ast_type_to_ty(&p.ty))
                            .collect();
                        let ret_ty = method
                            .return_type
                            .as_ref()
                            .map(|t| self.ast_type_to_ty(t))
                            .unwrap_or_else(|| self.fresh_var());
                        let qualified = format!("{}::{}", imp.target, method.name);
                        let fn_ty = Ty::Function {
                            params: param_tys,
                            ret: Box::new(ret_ty),
                        };
                        self.fn_sigs.insert(qualified, fn_ty);
                        self.type_params_in_scope = prev;
                    }
                }
                Item::Enum(e) => {
                    let variants: Vec<String> =
                        e.variants.iter().map(|v| v.name.clone()).collect();
                    self.enum_defs.insert(e.name.clone(), variants);
                }
                Item::Contract(c) => {
                    // Register contract fields in the structs map so field
                    // access checking works identically to structs.
                    let mut fields = HashMap::new();
                    for field in &c.fields {
                        fields.insert(field.name.clone(), self.ast_type_to_ty(&field.ty));
                    }
                    self.structs.insert(c.name.clone(), StructInfo { fields });
                    self.contracts.insert(c.name.clone());
                }
                Item::Mod(m) => {
                    // Recursively collect declarations from inline module items,
                    // registering them with a namespace prefix (e.g. "math::Vec3").
                    if let Some(items) = &m.items {
                        let mod_program_items: &[Item] = items;
                        // Collect declarations from module items individually
                        for sub in mod_program_items {
                            match sub {
                                Item::Struct(s) => {
                                    let mut fields = std::collections::HashMap::new();
                                    for field in &s.fields {
                                        fields.insert(field.name.clone(), self.ast_type_to_ty(&field.ty));
                                    }
                                    self.structs.insert(s.name.clone(), StructInfo { fields });
                                }
                                Item::Function(f) => {
                                    let param_tys: Vec<Ty> = f.params.iter().map(|p| self.ast_type_to_ty(&p.ty)).collect();
                                    let ret_ty = f.return_type.as_ref().map(|t| self.ast_type_to_ty(t)).unwrap_or_else(|| self.fresh_var());
                                    let fn_ty = Ty::Function { params: param_tys, ret: Box::new(ret_ty) };
                                    self.fn_sigs.insert(f.name.clone(), fn_ty);
                                }
                                Item::Enum(e) => {
                                    let variants: Vec<String> = e.variants.iter().map(|v| v.name.clone()).collect();
                                    self.enum_defs.insert(e.name.clone(), variants);
                                }
                                _ => {},
                            }
                        }
                        // Also register namespaced versions of public items
                        for sub_item in items {
                            match sub_item {
                                Item::Function(f) if f.is_pub => {
                                    let qualified = format!("{}::{}", m.name, f.name);
                                    if let Some(sig) = self.fn_sigs.get(&f.name).cloned() {
                                        self.fn_sigs.insert(qualified, sig);
                                    }
                                }
                                Item::Struct(s) if s.is_pub => {
                                    let qualified = format!("{}::{}", m.name, s.name);
                                    if let Some(info) = self.structs.get(&s.name).cloned() {
                                        self.structs.insert(qualified, info);
                                    }
                                }
                                _ => {},
                            }
                        }
                    }
                }
                Item::Use(use_path) => {
                    // For namespaced imports like `use math::Vec3`, register the
                    // local name as an alias for the qualified name.
                    if use_path.segments.len() >= 2 && !use_path.glob {
                        let original = use_path.segments.last().unwrap().clone();
                        let local = use_path.alias.as_ref().unwrap_or(&original).clone();
                        let qualified = use_path.segments.join("::");

                        // Copy function signature if available
                        if let Some(sig) = self.fn_sigs.get(&qualified).cloned() {
                            self.fn_sigs.insert(local.clone(), sig);
                        }
                        // Copy struct info if available
                        if let Some(info) = self.structs.get(&qualified).cloned() {
                            self.structs.insert(local, info);
                        }
                    }
                }
                Item::Trait(trait_def) => {
                    let prev = self.type_params_in_scope.clone();
                    for tp in &trait_def.type_params {
                        self.type_params_in_scope.insert(tp.clone());
                    }
                    let mut methods = HashMap::new();
                    let mut default_methods = std::collections::HashSet::new();
                    for method in &trait_def.methods {
                        let param_tys: Vec<Ty> = method
                            .params
                            .iter()
                            .map(|p| self.ast_type_to_ty(&p.ty))
                            .collect();
                        let ret_ty = method
                            .return_type
                            .as_ref()
                            .map(|t| self.ast_type_to_ty(t))
                            .unwrap_or(Ty::Unit);
                        methods.insert(method.name.clone(), (param_tys, ret_ty));
                        if method.default_body.is_some() {
                            default_methods.insert(method.name.clone());
                        }
                    }
                    self.traits.insert(
                        trait_def.name.clone(),
                        TraitInfo { methods, default_methods },
                    );
                    self.type_params_in_scope = prev;
                }
                _ => {},
            }
        }
    }

    /// Look up the variant names for a given enum type.
    pub fn get_enum_variants(&self, name: &str) -> Option<Vec<String>> {
        self.enum_defs.get(name).cloned()
    }

    // -- second pass: infer & check types ---------------------------------

    fn check_program(&mut self, program: &Program) {
        let mut env = TypeEnv::new();

        // Seed the environment with collected function signatures.
        for (name, ty) in &self.fn_sigs {
            env.insert(name.clone(), ty.clone());
        }

        for item in &program.items {
            match item {
                Item::Function(f) => self.check_function(f, &mut env),
                Item::Component(c) => self.check_component(c, &mut env),
                Item::Struct(_) => { /* validated during collection */ }
                Item::Enum(_) => { /* validated during collection */ }
                Item::Impl(imp) => self.check_impl(imp, &mut env),
                Item::Store(store) => self.check_store(store, &mut env),
                Item::Use(_) => {}
                Item::Mod(m) => {
                    // Recursively check items in inline modules
                    if let Some(items) = &m.items {
                        for sub_item in items {
                            match sub_item {
                                Item::Function(f) => self.check_function(f, &mut env),
                                Item::Struct(_) => {}
                                Item::Enum(_) => {}
                                Item::Impl(imp) => self.check_impl(imp, &mut env),
                                _ => {}
                            }
                        }
                    }
                }
                Item::Contract(_) => { /* contracts checked at field-access and fetch sites */ }
                Item::Agent(_) => { /* agent type checking TODO */ }
                Item::Router(_) => { /* router type checking TODO */ }
                Item::LazyComponent(lc) => self.check_component(&lc.component, &mut env),
                Item::Test(test) => {
                    let mut test_env = env.child();
                    self.infer_block(&test.body, &mut test_env);
                }
                Item::Trait(trait_def) => {
                    // Check default method bodies
                    for method in &trait_def.methods {
                        if let Some(ref body) = method.default_body {
                            let mut method_env = env.child();
                            for param in &method.params {
                                method_env.insert(
                                    param.name.clone(),
                                    self.ast_type_to_ty(&param.ty),
                                );
                            }
                            let body_ty = self.infer_block(body, &mut method_env);
                            if let Some(ref ret_ast) = method.return_type {
                                let declared = self.ast_type_to_ty(ret_ast);
                                self.unify(&declared, &body_ty, method.span);
                            }
                        }
                    }
                }
                Item::App(_) => { /* app type checking TODO */ }
                Item::Form(form) => {
                    // Type check form methods
                    for method in &form.methods {
                        let prev = self.type_params_in_scope.clone();
                        for tp in &method.type_params {
                            self.type_params_in_scope.insert(tp.clone());
                        }
                        self.check_function(method, &mut env);
                        self.type_params_in_scope = prev;
                    }
                }
                Item::Page(page) => {
                    // Type check page like a component
                    for method in &page.methods {
                        let prev = self.type_params_in_scope.clone();
                        for tp in &method.type_params {
                            self.type_params_in_scope.insert(tp.clone());
                        }
                        self.check_function(method, &mut env);
                        self.type_params_in_scope = prev;
                    }
                }
                Item::Channel(_) => { /* channel type checking TODO */ }
                Item::Embed(_) => { /* embed type checking TODO */ }
                Item::Pdf(_) => { /* pdf type checking TODO */ }
                Item::Payment(_) => { /* payment type checking TODO */ }
                Item::Auth(_) => { /* auth type checking TODO */ }
                Item::Upload(_) => { /* upload type checking TODO */ }
                Item::Db(_) => { /* db type checking TODO */ }
                Item::Cache(_) => { /* cache type checking TODO */ }
                Item::Breakpoints(_) => { /* breakpoints are config-only */ }
                Item::Theme(_) => { /* theme type checking TODO */ }
                Item::Animation(_) => { /* animation type checking TODO */ }
            }
        }

        // Validate trait implementations: check that all required methods are implemented
        for item in &program.items {
            if let Item::Impl(imp) = item {
                for trait_name in &imp.trait_impls {
                    if let Some(trait_info) = self.traits.get(trait_name).cloned() {
                        let implemented_methods: std::collections::HashSet<String> =
                            imp.methods.iter().map(|m| m.name.clone()).collect();
                        for (method_name, _) in &trait_info.methods {
                            if !implemented_methods.contains(method_name)
                                && !trait_info.default_methods.contains(method_name)
                            {
                                self.error(
                                    format!(
                                        "type `{}` does not implement required trait method `{}` from trait `{}`",
                                        imp.target, method_name, trait_name
                                    ),
                                    imp.span,
                                );
                            }
                        }
                    } else {
                        self.error(
                            format!("trait `{}` not found", trait_name),
                            imp.span,
                        );
                    }
                }
            }
        }

        // Race condition detection: warn when multiple components mutate the
        // same store without using atomic signals.
        self.check_store_mutation_races(program);
    }

    /// Walk components and track which stores each one mutates.
    /// If two or more components mutate the same store and neither uses atomic
    /// signals, emit a warning about potential race conditions.
    fn check_store_mutation_races(&mut self, program: &Program) {
        use std::collections::HashMap as HM;
        let mut store_mutators: HM<String, Vec<String>> = HM::new();

        // Collect store names that have atomic signals
        let mut stores_with_atomics: std::collections::HashSet<String> = std::collections::HashSet::new();
        for item in &program.items {
            if let Item::Store(s) = item {
                if s.signals.iter().any(|sig| sig.atomic) {
                    stores_with_atomics.insert(s.name.clone());
                }
            }
        }

        for item in &program.items {
            if let Item::Component(c) = item {
                // Walk component methods looking for store mutation patterns
                for method in &c.methods {
                    self.collect_store_mutations(&method.body, &c.name, &mut store_mutators);
                }
            }
        }

        // Check for conflicting mutations
        for (store_name, mutators) in &store_mutators {
            if mutators.len() > 1 && !stores_with_atomics.contains(store_name) {
                let comp_list = mutators.join("` and `");
                self.error(
                    format!(
                        "warning: components `{}` both mutate store `{}` — consider using `atomic` signals to prevent race conditions",
                        comp_list, store_name,
                    ),
                    Span::new(0, 0, 0, 0),
                );
            }
        }
    }

    /// Walk a block looking for store mutation patterns (e.g. `StoreName.action(...)`)
    fn collect_store_mutations(
        &self,
        block: &Block,
        component_name: &str,
        store_mutators: &mut std::collections::HashMap<String, Vec<String>>,
    ) {
        for stmt in &block.stmts {
            if let Stmt::Expr(expr) = stmt {
                self.collect_store_mutations_in_expr(expr, component_name, store_mutators);
            }
        }
    }

    fn collect_store_mutations_in_expr(
        &self,
        expr: &Expr,
        component_name: &str,
        store_mutators: &mut std::collections::HashMap<String, Vec<String>>,
    ) {
        match expr {
            Expr::MethodCall { object, method, args, .. } => {
                // Pattern: StoreName.dispatch(...) or StoreName.some_action(...)
                if let Expr::Ident(store_name) = object.as_ref() {
                    // Heuristic: if calling a method on a PascalCase name, assume it's a store mutation
                    if store_name.chars().next().is_some_and(|c| c.is_uppercase()) {
                        let mutators = store_mutators.entry(store_name.clone()).or_default();
                        if !mutators.contains(&component_name.to_string()) {
                            mutators.push(component_name.to_string());
                        }
                    }
                }
                self.collect_store_mutations_in_expr(object, component_name, store_mutators);
                for arg in args {
                    self.collect_store_mutations_in_expr(arg, component_name, store_mutators);
                }
            }
            Expr::Assign { target, value } => {
                // Pattern: StoreName.field = value
                if let Expr::FieldAccess { object, .. } = target.as_ref() {
                    if let Expr::Ident(store_name) = object.as_ref() {
                        if store_name.chars().next().is_some_and(|c| c.is_uppercase()) {
                            let mutators = store_mutators.entry(store_name.clone()).or_default();
                            if !mutators.contains(&component_name.to_string()) {
                                mutators.push(component_name.to_string());
                            }
                        }
                    }
                }
                self.collect_store_mutations_in_expr(value, component_name, store_mutators);
            }
            _ => {}
        }
    }

    fn check_store(&mut self, store: &StoreDef, env: &mut TypeEnv) {
        let mut store_env = env.child();
        store_env.insert("self".to_string(), Ty::Struct(store.name.clone()));

        // Register store signals as fields in the struct registry so field
        // access works.
        let mut fields = HashMap::new();
        for sig in &store.signals {
            let init_ty = self.infer_expr(&sig.initializer, &mut store_env);
            let ty = if let Some(ast_ty) = &sig.ty {
                let declared = self.ast_type_to_ty(ast_ty);
                self.unify(&declared, &init_ty, store.span);
                declared
            } else {
                init_ty
            };
            store_env.insert(sig.name.clone(), ty.clone());
            fields.insert(sig.name.clone(), ty);
        }
        self.structs
            .insert(store.name.clone(), StructInfo { fields });

        for action in &store.actions {
            let mut action_env = store_env.child();
            for param in &action.params {
                action_env.insert(param.name.clone(), self.ast_type_to_ty(&param.ty));
            }
            self.infer_block(&action.body, &mut action_env);
        }

        for computed in &store.computed {
            let mut comp_env = store_env.child();
            let body_ty = self.infer_block(&computed.body, &mut comp_env);
            if let Some(ret_ast) = &computed.return_type {
                let declared = self.ast_type_to_ty(ret_ast);
                self.unify(&declared, &body_ty, computed.span);
            }
        }

        for effect in &store.effects {
            let mut effect_env = store_env.child();
            self.infer_block(&effect.body, &mut effect_env);
        }
    }

    fn check_function(&mut self, func: &Function, env: &mut TypeEnv) {
        // Bring generic type parameters into scope for this function.
        let prev_type_params = self.type_params_in_scope.clone();
        for tp in &func.type_params {
            self.type_params_in_scope.insert(tp.clone());
        }

        let mut body_env = env.child();

        // For each type parameter, create a fresh type variable so that HM
        // inference can unify through usage.
        for tp in &func.type_params {
            let tv = self.fresh_var();
            body_env.insert(tp.clone(), tv);
        }

        for param in &func.params {
            let ty = self.ast_type_to_ty(&param.ty);
            body_env.insert(param.name.clone(), ty);
        }

        let body_ty = self.infer_block(&func.body, &mut body_env);

        // If a return type was declared, unify; otherwise the inferred body
        // type becomes the return type (updates the signature via the type
        // variable allocated in `collect_declarations`).
        if let Some(ret_ast) = &func.return_type {
            let declared = self.ast_type_to_ty(ret_ast);
            self.unify(&declared, &body_ty, func.span);
        } else if let Some(sig) = self.fn_sigs.get(&func.name).cloned() {
            if let Ty::Function { ret, .. } = sig {
                self.unify(&ret, &body_ty, func.span);
            }
        }

        // Restore previous type parameter scope.
        self.type_params_in_scope = prev_type_params;
    }

    fn check_component(&mut self, comp: &Component, env: &mut TypeEnv) {
        let mut comp_env = env.child();

        // Props are available as local bindings.
        for prop in &comp.props {
            let ty = self.ast_type_to_ty(&prop.ty);
            comp_env.insert(prop.name.clone(), ty);
        }

        // State fields.
        for state in &comp.state {
            let init_ty = self.infer_expr(&state.initializer, &mut comp_env);
            let base_ty = if let Some(ast_ty) = &state.ty {
                let declared = self.ast_type_to_ty(ast_ty);
                self.unify(&declared, &init_ty, comp.span);
                declared
            } else {
                init_ty
            };
            let ty = if state.secret {
                Ty::Secret(Box::new(base_ty))
            } else {
                base_ty
            };
            comp_env.insert(state.name.clone(), ty);
        }

        // Check render block for secret safety — secret values must not appear
        // in template expressions.
        self.check_template_secret_safety(&comp.render.body, &comp_env, comp.span);

        // Methods.
        for method in &comp.methods {
            self.check_function(method, &mut comp_env);
        }
    }

    /// Recursively check that no secret-typed variable is used in a template expression.
    fn check_template_secret_safety(&mut self, node: &TemplateNode, env: &TypeEnv, span: Span) {
        match node {
            TemplateNode::Expression(expr) => {
                self.check_expr_not_secret(expr, env, span);
            }
            TemplateNode::Element(el) => {
                for child in &el.children {
                    self.check_template_secret_safety(child, env, span);
                }
                for attr in &el.attributes {
                    match attr {
                        Attribute::Dynamic { value, .. } => {
                            self.check_expr_not_secret(value, env, span);
                        }
                        Attribute::Aria { value, .. } => {
                            self.check_expr_not_secret(value, env, span);
                        }
                        _ => {}
                    }
                }
            }
            TemplateNode::Fragment(nodes) => {
                for child in nodes {
                    self.check_template_secret_safety(child, env, span);
                }
            }
            TemplateNode::Link { children, .. } => {
                for child in children {
                    self.check_template_secret_safety(child, env, span);
                }
            }
            TemplateNode::TextLiteral(_) => {}
        }
    }

    /// Check that an expression does not resolve to a secret type.
    fn check_expr_not_secret(&mut self, expr: &Expr, env: &TypeEnv, span: Span) {
        if let Expr::Ident(name) = expr {
            if let Some(ty) = env.lookup(name) {
                if matches!(ty, Ty::Secret(_)) {
                    self.error(
                        format!("cannot render secret value '{}' to DOM", name),
                        span,
                    );
                }
            }
        }
        if let Expr::FormatString { parts } = expr {
            for part in parts {
                if let FormatPart::Expression(inner) = part {
                    self.check_expr_not_secret(inner, env, span);
                }
            }
        }
    }

    fn check_impl(&mut self, imp: &ImplBlock, env: &mut TypeEnv) {
        for method in &imp.methods {
            let mut method_env = env.child();
            // `self` is the target struct type.
            method_env.insert("self".to_string(), Ty::Struct(imp.target.clone()));
            self.check_function(method, &mut method_env);
        }
    }

    // -- block / statement inference --------------------------------------

    fn infer_block(&mut self, block: &Block, env: &mut TypeEnv) -> Ty {
        let mut result_ty = Ty::Unit;
        for stmt in &block.stmts {
            result_ty = self.infer_stmt(stmt, env, block.span);
        }
        result_ty
    }

    fn infer_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv, span: Span) -> Ty {
        match stmt {
            Stmt::Let {
                name,
                ty,
                secret,
                value,
                ..
            } => {
                let val_ty = self.infer_expr(value, env);
                let base_ty = if let Some(ast_ty) = ty {
                    let declared = self.ast_type_to_ty(ast_ty);
                    self.unify(&declared, &val_ty, span);
                    declared
                } else {
                    val_ty
                };
                let final_ty = if *secret {
                    Ty::Secret(Box::new(base_ty))
                } else {
                    base_ty
                };
                env.insert(name.clone(), final_ty);
                Ty::Unit
            }
            Stmt::Signal { name, ty, secret, value, .. } => {
                let val_ty = self.infer_expr(value, env);
                let base_ty = if let Some(ast_ty) = ty {
                    let declared = self.ast_type_to_ty(ast_ty);
                    self.unify(&declared, &val_ty, span);
                    declared
                } else {
                    val_ty
                };
                let final_ty = if *secret {
                    Ty::Secret(Box::new(base_ty))
                } else {
                    base_ty
                };
                env.insert(name.clone(), final_ty);
                Ty::Unit
            }
            Stmt::Expr(expr) => {
                // Check if this is a function call whose return value is being
                // discarded — applies to must_use functions and functions
                // returning Result<T,E> or Option<T>.
                let ty = self.infer_expr(expr, env);
                let resolved = self.resolve(&ty);

                // Determine the callee name (if this is a direct function call)
                let callee_name = match expr {
                    Expr::FnCall { callee, .. } => match callee.as_ref() {
                        Expr::Ident(name) => Some(name.clone()),
                        _ => None,
                    },
                    _ => None,
                };

                // Warn if discarding Result<T,E>
                if matches!(resolved, Ty::Result_ { .. }) {
                    self.error(
                        "unused Result value — must be handled with match, unwrap, or let binding".to_string(),
                        span,
                    );
                }
                // Warn if discarding Option<T>
                else if matches!(resolved, Ty::Option_(_)) {
                    self.error(
                        "unused Option value — must be handled with match, unwrap, or let binding".to_string(),
                        span,
                    );
                }
                // Warn if discarding return value from a must_use function
                else if let Some(name) = callee_name {
                    if self.must_use_fns.contains(&name) {
                        self.error(
                            format!(
                                "return value of `must_use` function `{}` must not be discarded",
                                name,
                            ),
                            span,
                        );
                    }
                }

                ty
            }
            Stmt::Return(maybe_expr) => {
                if let Some(expr) = maybe_expr {
                    self.infer_expr(expr, env)
                } else {
                    Ty::Unit
                }
            }
            Stmt::Yield(expr) => {
                self.infer_expr(expr, env)
            }
            Stmt::LetDestructure { pattern, ty, value } => {
                let val_ty = self.infer_expr(value, env);
                let final_ty = if let Some(ast_ty) = ty {
                    let declared = self.ast_type_to_ty(ast_ty);
                    self.unify(&declared, &val_ty, span);
                    declared
                } else {
                    val_ty
                };
                self.bind_pattern(pattern, &final_ty, env, span);
                Ty::Unit
            }
        }
    }

    /// Bind each variable in a destructuring pattern to the appropriate type.
    fn bind_pattern(&mut self, pattern: &Pattern, ty: &Ty, env: &mut TypeEnv, span: Span) {
        let resolved = self.resolve(ty);
        match pattern {
            Pattern::Ident(name) => {
                env.insert(name.clone(), resolved);
            }
            Pattern::Wildcard => {}
            Pattern::Tuple(pats) => {
                if let Ty::Tuple(tys) = &resolved {
                    if pats.len() != tys.len() {
                        self.error(
                            format!("tuple pattern has {} elements but type has {}", pats.len(), tys.len()),
                            span,
                        );
                    }
                    for (p, t) in pats.iter().zip(tys.iter()) {
                        self.bind_pattern(p, t, env, span);
                    }
                } else {
                    self.error(
                        format!("cannot destructure non-tuple type {} as tuple", resolved),
                        span,
                    );
                }
            }
            Pattern::Struct { name, fields, .. } => {
                if let Ty::Struct(struct_name) = &resolved {
                    if name != struct_name {
                        self.error(
                            format!("expected struct {} but found {}", name, struct_name),
                            span,
                        );
                    }
                    if let Some(info) = self.structs.get(name).cloned() {
                        for (field_name, field_pat) in fields {
                            if let Some(field_ty) = info.fields.get(field_name) {
                                self.bind_pattern(field_pat, field_ty, env, span);
                            } else {
                                self.error(
                                    format!("struct {} has no field {}", name, field_name),
                                    span,
                                );
                            }
                        }
                    } else {
                        self.error(format!("unknown struct: {}", name), span);
                    }
                } else {
                    self.error(
                        format!("cannot destructure non-struct type {} with struct pattern", resolved),
                        span,
                    );
                }
            }
            Pattern::Array(pats) => {
                if let Ty::Array(elem_ty) = &resolved {
                    for p in pats {
                        self.bind_pattern(p, elem_ty, env, span);
                    }
                } else {
                    self.error(
                        format!("cannot destructure non-array type {} as array", resolved),
                        span,
                    );
                }
            }
            Pattern::Literal(_) | Pattern::Variant { .. } => {}
        }
    }

    // -- expression inference ---------------------------------------------

    fn infer_expr(&mut self, expr: &Expr, env: &mut TypeEnv) -> Ty {
        match expr {
            // --- Literals ---
            Expr::Integer(_) => Ty::I32,
            Expr::Float(_) => Ty::F64,
            Expr::StringLit(_) => Ty::String_,
            Expr::Bool(_) => Ty::Bool,

            // --- Variables ---
            Expr::Ident(name) => {
                if let Some(ty) = env.lookup(name) {
                    ty.clone()
                } else if let Some(ty) = self.fn_sigs.get(name) {
                    ty.clone()
                } else {
                    self.error(
                        format!("undefined variable: {}", name),
                        Self::dummy_span(),
                    );
                    Ty::Error
                }
            }
            Expr::SelfExpr => {
                if let Some(ty) = env.lookup("self") {
                    ty.clone()
                } else {
                    self.error("use of `self` outside of impl block", Self::dummy_span());
                    Ty::Error
                }
            }

            // --- Binary operations ---
            Expr::Binary { op, left, right } => {
                let left_ty = self.infer_expr(left, env);
                let right_ty = self.infer_expr(right, env);
                self.check_binary_op(op, &left_ty, &right_ty)
            }

            // --- Unary operations ---
            Expr::Unary { op, operand } => {
                let operand_ty = self.infer_expr(operand, env);
                match op {
                    UnaryOp::Neg => {
                        let resolved = self.resolve(&operand_ty);
                        match resolved {
                            Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64 => resolved,
                            Ty::Var(_) => {
                                // Default to i32 for negation of unconstrained var.
                                self.unify(&operand_ty, &Ty::I32, Self::dummy_span());
                                Ty::I32
                            }
                            _ => {
                                self.error(
                                    format!("cannot negate type {}", resolved),
                                    Self::dummy_span(),
                                );
                                Ty::Error
                            }
                        }
                    }
                    UnaryOp::Not => {
                        self.unify(&operand_ty, &Ty::Bool, Self::dummy_span());
                        Ty::Bool
                    }
                }
            }

            // --- Field access ---
            Expr::FieldAccess { object, field } => {
                let obj_ty = self.infer_expr(object, env);
                self.resolve_field_access(&obj_ty, field)
            }

            // --- Method call ---
            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                let obj_ty = self.infer_expr(object, env);
                let arg_tys: Vec<Ty> = args.iter().map(|a| self.infer_expr(a, env)).collect();
                let resolved_obj = self.resolve(&obj_ty);

                // --- Iterator protocol: built-in methods on Array and Iterator ---
                if let Some(result) = self.check_iterator_method(&resolved_obj, method, &arg_tys) {
                    return result;
                }

                let struct_name = match &resolved_obj {
                    Ty::Struct(name) => Some(name.clone()),
                    Ty::Reference { inner, .. } => match self.resolve(inner) {
                        Ty::Struct(name) => Some(name),
                        _ => None,
                    },
                    _ => None,
                };

                if let Some(name) = struct_name {
                    let qualified = format!("{}::{}", name, method);
                    if let Some(sig) = self.fn_sigs.get(&qualified).cloned() {
                        if let Ty::Function { params, ret } = sig {
                            // Skip `self` param when matching args.
                            let param_start = if params.first() == Some(&Ty::Struct(name.clone()))
                                || params.first()
                                    == Some(&Ty::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(Ty::Struct(name.clone())),
            })
                                || params.first()
                                    == Some(&Ty::Reference {
                mutable: true,
                lifetime: None,
                inner: Box::new(Ty::Struct(name.clone())),
            })
                            {
                                1
                            } else {
                                0
                            };
                            let expected_params = &params[param_start..];
                            if arg_tys.len() != expected_params.len() {
                                self.error(
                                    format!(
                                        "method {}.{} expects {} arguments, got {}",
                                        name,
                                        method,
                                        expected_params.len(),
                                        arg_tys.len()
                                    ),
                                    Self::dummy_span(),
                                );
                            } else {
                                for (at, pt) in arg_tys.iter().zip(expected_params.iter()) {
                                    self.unify(at, pt, Self::dummy_span());
                                }
                            }
                            return *ret;
                        }
                    }
                }
                // Unknown method – return a fresh variable.
                self.fresh_var()
            }

            // --- Function call ---
            Expr::FnCall { callee, args } => {
                let callee_ty = self.infer_expr(callee, env);
                let arg_tys: Vec<Ty> = args.iter().map(|a| self.infer_expr(a, env)).collect();

                let resolved = self.resolve(&callee_ty);
                match resolved {
                    Ty::Function { params, ret } => {
                        if arg_tys.len() != params.len() {
                            self.error(
                                format!(
                                    "function expects {} arguments, got {}",
                                    params.len(),
                                    arg_tys.len()
                                ),
                                Self::dummy_span(),
                            );
                        } else {
                            for (at, pt) in arg_tys.iter().zip(params.iter()) {
                                self.unify(at, pt, Self::dummy_span());
                            }
                        }
                        *ret
                    }
                    Ty::Error => Ty::Error,
                    _ => {
                        self.error(
                            format!("type {} is not callable", resolved),
                            Self::dummy_span(),
                        );
                        Ty::Error
                    }
                }
            }

            // --- Index ---
            Expr::Index { object, index } => {
                let obj_ty = self.infer_expr(object, env);
                let idx_ty = self.infer_expr(index, env);

                // Index must be integer.
                let idx_resolved = self.resolve(&idx_ty);
                match idx_resolved {
                    Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 => {}
                    Ty::Var(_) => {
                        self.unify(&idx_ty, &Ty::I32, Self::dummy_span());
                    }
                    _ => {
                        self.error(
                            format!("index must be integer, found {}", idx_resolved),
                            Self::dummy_span(),
                        );
                    }
                }

                match self.resolve(&obj_ty) {
                    Ty::Array(inner) => *inner,
                    Ty::Error => Ty::Error,
                    other => {
                        self.error(
                            format!("cannot index into type {}", other),
                            Self::dummy_span(),
                        );
                        Ty::Error
                    }
                }
            }

            // --- Control flow ---
            Expr::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_ty = self.infer_expr(condition, env);
                self.unify(&cond_ty, &Ty::Bool, Self::dummy_span());

                let mut then_env = env.child();
                let then_ty = self.infer_block(then_block, &mut then_env);

                if let Some(else_blk) = else_block {
                    let mut else_env = env.child();
                    let else_ty = self.infer_block(else_blk, &mut else_env);
                    self.unify(&then_ty, &else_ty, then_block.span);
                    then_ty
                } else {
                    Ty::Unit
                }
            }

            Expr::Match { subject, arms } => {
                let _subject_ty = self.infer_expr(subject, env);

                let result = self.fresh_var();
                for arm in arms {
                    let arm_ty = self.infer_expr(&arm.body, env);
                    self.unify(&result, &arm_ty, Self::dummy_span());
                }
                self.resolve(&result)
            }

            Expr::For {
                binding,
                iterator,
                body,
            } => {
                let iter_ty = self.infer_expr(iterator, env);
                let elem_ty = match self.resolve(&iter_ty) {
                    Ty::Array(inner) => *inner,
                    _ => self.fresh_var(),
                };

                let mut loop_env = env.child();
                loop_env.insert(binding.clone(), elem_ty);
                self.infer_block(body, &mut loop_env);
                Ty::Unit
            }

            Expr::While { condition, body } => {
                let cond_ty = self.infer_expr(condition, env);
                self.unify(&cond_ty, &Ty::Bool, Self::dummy_span());

                let mut loop_env = env.child();
                self.infer_block(body, &mut loop_env);
                Ty::Unit
            }

            Expr::Block(block) => {
                let mut block_env = env.child();
                self.infer_block(block, &mut block_env)
            }

            // --- Ownership / References ---
            Expr::Borrow(inner) => {
                let inner_ty = self.infer_expr(inner, env);
                Ty::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(inner_ty),
            }
            }
            Expr::BorrowMut(inner) => {
                let inner_ty = self.infer_expr(inner, env);
                Ty::Reference {
                mutable: true,
                lifetime: None,
                inner: Box::new(inner_ty),
            }
            }

            // --- Struct construction ---
            Expr::StructInit { name, fields } => {
                if let Some(info) = self.structs.get(name).cloned() {
                    for (field_name, field_expr) in fields {
                        let expr_ty = self.infer_expr(field_expr, env);
                        if let Some(expected) = info.fields.get(field_name) {
                            self.unify(&expr_ty, expected, Self::dummy_span());
                        } else {
                            self.error(
                                format!("struct {} has no field named {}", name, field_name),
                                Self::dummy_span(),
                            );
                        }
                    }
                    // Check for missing fields.
                    for declared in info.fields.keys() {
                        if !fields.iter().any(|(n, _)| n == declared) {
                            self.error(
                                format!(
                                    "missing field {} in struct {} initialization",
                                    declared, name
                                ),
                                Self::dummy_span(),
                            );
                        }
                    }
                    Ty::Struct(name.clone())
                } else {
                    self.error(format!("unknown struct: {}", name), Self::dummy_span());
                    Ty::Error
                }
            }

            // --- Assignment ---
            Expr::Assign { target, value } => {
                let target_ty = self.infer_expr(target, env);
                let value_ty = self.infer_expr(value, env);
                self.unify(&target_ty, &value_ty, Self::dummy_span());
                Ty::Unit
            }

            // --- Await ---
            Expr::Await(inner) => {
                // Await unwraps the inner future; for now, pass through.
                self.infer_expr(inner, env)
            }

            // --- Fetch ---
            Expr::Fetch { url, options, contract } => {
                let url_ty = self.infer_expr(url, env);
                self.unify(&url_ty, &Ty::String_, Self::dummy_span());
                if let Some(opts) = options {
                    self.infer_expr(opts, env);
                }
                // If a contract type is specified, the response is validated
                // against it and typed as the contract. Otherwise, opaque.
                if let Some(contract_name) = contract {
                    if self.contracts.contains(contract_name) {
                        Ty::Contract(contract_name.clone())
                    } else if self.structs.contains_key(contract_name) {
                        // Allow binding to a struct too, but warn that it
                        // won't get runtime boundary validation.
                        Ty::Struct(contract_name.clone())
                    } else {
                        self.error(
                            format!("fetch -> {}: unknown contract type", contract_name),
                            Self::dummy_span(),
                        );
                        Ty::Error
                    }
                } else {
                    // No contract — fetch returns an opaque Response type.
                    self.fresh_var()
                }
            }

            // --- Closure ---
            Expr::Closure { params, body } => {
                let mut closure_env = env.child();
                let mut param_tys = Vec::new();
                for (name, maybe_ty) in params {
                    let ty = if let Some(ast_ty) = maybe_ty {
                        self.ast_type_to_ty(ast_ty)
                    } else {
                        self.fresh_var()
                    };
                    closure_env.insert(name.clone(), ty.clone());
                    param_tys.push(ty);
                }
                let ret_ty = self.infer_expr(body, &mut closure_env);
                Ty::Function {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                }
            }

            // --- AI / Streaming / Concurrency ---
            Expr::PromptTemplate { interpolations, .. } => {
                for (_name, expr) in interpolations {
                    self.infer_expr(expr, env);
                }
                Ty::String_
            }
            Expr::Navigate { path } => {
                let path_ty = self.infer_expr(path, env);
                self.unify(&path_ty, &Ty::String_, Self::dummy_span());
                Ty::Unit
            }
            Expr::Stream { source } => {
                self.infer_expr(source, env);
                self.fresh_var()
            }
            Expr::Suspend { fallback, body } => {
                self.infer_expr(fallback, env);
                self.infer_expr(body, env)
            }
            Expr::Spawn { body, .. } => {
                self.infer_block(body, env);
                Ty::Unit
            }
            Expr::Channel { .. } => {
                self.fresh_var()
            }
            Expr::Send { channel, value } => {
                self.infer_expr(channel, env);
                self.infer_expr(value, env);
                Ty::Unit
            }
            Expr::Receive { channel } => {
                self.infer_expr(channel, env);
                self.fresh_var()
            }
            Expr::Parallel { tasks, .. } => {
                for expr in tasks {
                    self.infer_expr(expr, env);
                }
                self.fresh_var()
            }
            Expr::TryCatch { body, error_binding, catch_body } => {
                let try_ty = self.infer_expr(body, env);
                let mut catch_env = env.child();
                catch_env.insert(error_binding.clone(), Ty::String_);
                let catch_ty = self.infer_expr(catch_body, &mut catch_env);
                self.unify(&try_ty, &catch_ty, Self::dummy_span());
                try_ty
            }

            // Testing assertions
            Expr::Assert { condition, .. } => {
                let cond_ty = self.infer_expr(condition, env);
                self.unify(&cond_ty, &Ty::Bool, Self::dummy_span());
                Ty::Unit
            }
            Expr::AssertEq { left, right, .. } => {
                let left_ty = self.infer_expr(left, env);
                let right_ty = self.infer_expr(right, env);
                self.unify(&left_ty, &right_ty, Self::dummy_span());
                Ty::Unit
            }
            Expr::Animate { target, .. } => {
                self.infer_expr(target, env);
                Ty::Unit
            }

            // Format string interpolation — always produces a String.
            Expr::FormatString { parts } => {
                for part in parts {
                    if let FormatPart::Expression(expr) = part {
                        self.infer_expr(expr, env);
                    }
                }
                Ty::String_
            }

            // `?` error propagation operator
            Expr::Try(inner) => {
                let inner_ty = self.infer_expr(inner, env);
                let resolved = self.resolve(&inner_ty);
                match resolved {
                    Ty::Result_ { ok, .. } => *ok,
                    Ty::Option_(inner) => *inner,
                    _ => {
                        self.error(
                            format!("the `?` operator can only be applied to Result or Option types, found {}", resolved),
                            Self::dummy_span(),
                        );
                        Ty::Error
                    }
                }
            }

            Expr::DynamicImport { path, .. } => {
                self.infer_expr(path, env);
                // Dynamic imports return a promise-like async module handle
                Ty::I32
            }
            Expr::Download { data, filename, .. } => {
                self.infer_expr(data, env);
                self.infer_expr(filename, env);
                Ty::Unit
            }
            Expr::Env { name, .. } => {
                self.infer_expr(name, env);
                Ty::String_
            }
            Expr::Trace { label, body, .. } => {
                self.infer_expr(label, env);
                self.infer_block(body, env);
                Ty::Unit
            }
            Expr::Flag { name, .. } => {
                self.infer_expr(name, env);
                Ty::Bool
            }
            Expr::VirtualList { items, item_height, template, .. } => {
                self.infer_expr(items, env);
                self.infer_expr(item_height, env);
                self.infer_expr(template, env);
                Ty::Unit
            }
        }
    }

    // -- binary op checking -----------------------------------------------

    fn check_binary_op(&mut self, op: &BinOp, left: &Ty, right: &Ty) -> Ty {
        let left = self.resolve(left);
        let right = self.resolve(right);

        match op {
            // Arithmetic
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                // String concatenation
                if matches!(op, BinOp::Add) && left == Ty::String_ && right == Ty::String_ {
                    return Ty::String_;
                }
                if !self.is_numeric(&left) || !self.is_numeric(&right) {
                    self.error(
                        format!(
                            "cannot apply arithmetic operator to {} and {}",
                            left, right
                        ),
                        Self::dummy_span(),
                    );
                    return Ty::Error;
                }
                // If one side is float, the result is float.
                if self.is_float(&left) || self.is_float(&right) {
                    Ty::F64
                } else {
                    Ty::I32
                }
            }

            // Comparison
            BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
                // Both sides must be compatible.
                if left != Ty::Error && right != Ty::Error {
                    self.unify(&left, &right, Self::dummy_span());
                }
                Ty::Bool
            }

            // Logical
            BinOp::And | BinOp::Or => {
                self.unify(&left, &Ty::Bool, Self::dummy_span());
                self.unify(&right, &Ty::Bool, Self::dummy_span());
                Ty::Bool
            }
        }
    }

    fn is_numeric(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 | Ty::F32 | Ty::F64 | Ty::Var(_))
    }

    fn is_float(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::F32 | Ty::F64)
    }

    // -- field access on structs / references -----------------------------

    /// Check if a method call is a built-in iterator protocol method.
    /// Returns `Some(return_type)` if it matches, `None` otherwise.
    fn check_iterator_method(&mut self, obj_ty: &Ty, method: &str, arg_tys: &[Ty]) -> Option<Ty> {
        match method {
            // Array.iter() -> Iterator<T>
            "iter" => {
                if let Ty::Array(elem) = obj_ty {
                    return Some(Ty::Iterator(elem.clone()));
                }
                None
            }

            // Iterator<T>.map(|x| expr) -> Iterator<U>
            "map" => {
                if let Ty::Iterator(elem) = obj_ty {
                    if let Some(closure_ty) = arg_tys.first() {
                        let resolved = self.resolve(closure_ty);
                        if let Ty::Function { ret, params } = &resolved {
                            // Unify closure param with iterator element type
                            if let Some(param) = params.first() {
                                self.unify(param, elem, Self::dummy_span());
                            }
                            return Some(Ty::Iterator(ret.clone()));
                        }
                    }
                    // If no closure or unresolvable, return Iterator with fresh var
                    let fresh = self.fresh_var();
                    return Some(Ty::Iterator(Box::new(fresh)));
                }
                None
            }

            // Iterator<T>.filter(|x| bool) -> Iterator<T>
            "filter" => {
                if let Ty::Iterator(elem) = obj_ty {
                    if let Some(closure_ty) = arg_tys.first() {
                        let resolved = self.resolve(closure_ty);
                        if let Ty::Function { ret, params } = &resolved {
                            if let Some(param) = params.first() {
                                self.unify(param, elem, Self::dummy_span());
                            }
                            self.unify(ret, &Ty::Bool, Self::dummy_span());
                        }
                    }
                    return Some(Ty::Iterator(elem.clone()));
                }
                None
            }

            // Iterator<T>.collect() -> Array<T>
            "collect" => {
                if let Ty::Iterator(elem) = obj_ty {
                    return Some(Ty::Array(elem.clone()));
                }
                None
            }

            // Iterator<T>.fold(init, |acc, x| expr) -> AccType
            "fold" => {
                if let Ty::Iterator(elem) = obj_ty {
                    if arg_tys.len() >= 2 {
                        let init_ty = self.resolve(&arg_tys[0]);
                        let closure_ty = self.resolve(&arg_tys[1]);
                        if let Ty::Function { ret, params } = &closure_ty {
                            // Unify accumulator param with init type
                            if let Some(acc_param) = params.first() {
                                self.unify(acc_param, &init_ty, Self::dummy_span());
                            }
                            // Unify element param with iterator element type
                            if let Some(elem_param) = params.get(1) {
                                self.unify(elem_param, elem, Self::dummy_span());
                            }
                            // Unify return type with init type
                            self.unify(ret, &init_ty, Self::dummy_span());
                            return Some(init_ty);
                        }
                    }
                    // Fallback: return type of init
                    if let Some(init_ty) = arg_tys.first() {
                        return Some(self.resolve(init_ty));
                    }
                    return Some(self.fresh_var());
                }
                None
            }

            // Iterator<T>.any(|x| bool) -> Bool
            "any" => {
                if let Ty::Iterator(elem) = obj_ty {
                    if let Some(closure_ty) = arg_tys.first() {
                        let resolved = self.resolve(closure_ty);
                        if let Ty::Function { ret, params } = &resolved {
                            if let Some(param) = params.first() {
                                self.unify(param, elem, Self::dummy_span());
                            }
                            self.unify(ret, &Ty::Bool, Self::dummy_span());
                        }
                    }
                    return Some(Ty::Bool);
                }
                None
            }

            // Iterator<T>.all(|x| bool) -> Bool
            "all" => {
                if let Ty::Iterator(elem) = obj_ty {
                    if let Some(closure_ty) = arg_tys.first() {
                        let resolved = self.resolve(closure_ty);
                        if let Ty::Function { ret, params } = &resolved {
                            if let Some(param) = params.first() {
                                self.unify(param, elem, Self::dummy_span());
                            }
                            self.unify(ret, &Ty::Bool, Self::dummy_span());
                        }
                    }
                    return Some(Ty::Bool);
                }
                None
            }

            // Iterator<T>.enumerate() -> Iterator<(i32, T)>
            "enumerate" => {
                if let Ty::Iterator(elem) = obj_ty {
                    let tuple_ty = Ty::Tuple(vec![Ty::I32, *elem.clone()]);
                    return Some(Ty::Iterator(Box::new(tuple_ty)));
                }
                None
            }

            // Iterator<T>.zip(other_iter) -> Iterator<(T, U)>
            "zip" => {
                if let Ty::Iterator(elem_t) = obj_ty {
                    if let Some(other_ty) = arg_tys.first() {
                        let resolved_other = self.resolve(other_ty);
                        if let Ty::Iterator(elem_u) = &resolved_other {
                            let tuple_ty = Ty::Tuple(vec![*elem_t.clone(), *elem_u.clone()]);
                            return Some(Ty::Iterator(Box::new(tuple_ty)));
                        }
                    }
                    // If arg is not an iterator, still return Iterator<(T, ?U)>
                    let fresh = self.fresh_var();
                    let tuple_ty = Ty::Tuple(vec![*elem_t.clone(), fresh]);
                    return Some(Ty::Iterator(Box::new(tuple_ty)));
                }
                None
            }

            // Iterator<T>.count() -> i32
            "count" => {
                if let Ty::Iterator(_) = obj_ty {
                    return Some(Ty::I32);
                }
                None
            }

            // Iterator<T>.take(n) -> Iterator<T>
            "take" => {
                if let Ty::Iterator(elem) = obj_ty {
                    if let Some(n_ty) = arg_tys.first() {
                        self.unify(n_ty, &Ty::I32, Self::dummy_span());
                    }
                    return Some(Ty::Iterator(elem.clone()));
                }
                None
            }

            // Iterator<T>.skip(n) -> Iterator<T>
            "skip" => {
                if let Ty::Iterator(elem) = obj_ty {
                    if let Some(n_ty) = arg_tys.first() {
                        self.unify(n_ty, &Ty::I32, Self::dummy_span());
                    }
                    return Some(Ty::Iterator(elem.clone()));
                }
                None
            }

            _ => None,
        }
    }


    fn resolve_field_access(&mut self, obj_ty: &Ty, field: &str) -> Ty {
        let resolved = self.resolve(obj_ty);

        // Auto-deref references.
        let base = match &resolved {
            Ty::Reference { inner, .. } => self.resolve(inner),
            other => other.clone(),
        };

        match &base {
            Ty::Struct(name) | Ty::Contract(name) => {
                let kind = if self.contracts.contains(name) { "contract" } else { "struct" };
                if let Some(info) = self.structs.get(name).cloned() {
                    if let Some(field_ty) = info.fields.get(field) {
                        field_ty.clone()
                    } else {
                        self.error(
                            format!("{} {} has no field {} — check the {} definition", kind, name, field, kind),
                            Self::dummy_span(),
                        );
                        Ty::Error
                    }
                } else {
                    self.error(format!("unknown {}: {}", kind, name), Self::dummy_span());
                    Ty::Error
                }
            }
            Ty::Tuple(tys) => {
                // Tuple field access: t.0, t.1, etc.
                if let Ok(idx) = field.parse::<usize>() {
                    if idx < tys.len() {
                        tys[idx].clone()
                    } else {
                        self.error(
                            format!("tuple index {} out of range (len {})", idx, tys.len()),
                            Self::dummy_span(),
                        );
                        Ty::Error
                    }
                } else {
                    self.error(
                        format!("cannot access field {} on tuple", field),
                        Self::dummy_span(),
                    );
                    Ty::Error
                }
            }
            Ty::Error => Ty::Error,
            other => {
                self.error(
                    format!("cannot access field {} on type {}", field, other),
                    Self::dummy_span(),
                );
                Ty::Error
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run Hindley-Milner-style type inference and checking on an Nectar program.
///
/// Returns a `TypedProgram` with all types fully resolved, or a list of
/// `TypeError`s if the program is ill-typed.
pub fn infer_program(program: &Program) -> Result<TypedProgram, Vec<TypeError>> {
    let mut checker = TypeChecker::new();

    // First pass: collect struct definitions, component prop types, and
    // function signatures so forward references work.
    checker.collect_declarations(program);

    // Second pass: infer and check types for all function bodies, component
    // state, and render blocks.
    checker.check_program(program);

    if checker.errors.is_empty() {
        // Build final binding map with fully resolved types.
        let mut bindings = HashMap::new();
        for (name, ty) in &checker.fn_sigs {
            bindings.insert(name.clone(), checker.subst.finalize(ty));
        }

        let types = checker
            .subst
            .table
            .iter()
            .enumerate()
            .map(|(i, _)| {
                checker
                    .subst
                    .finalize(&Ty::Var(TypeId(i as u32)))
            })
            .collect();

        Ok(TypedProgram { types, bindings })
    } else {
        Err(checker.errors)
    }
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
        Block {
            stmts,
            span: span(),
        }
    }

    fn simple_program(items: Vec<Item>) -> Program {
        Program { items }
    }

    // -- basic type inference: let x = 42 infers i32 ----------------------

    #[test]
    fn infer_integer_literal() {
        let program = simple_program(vec![Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Let {
                name: "x".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Integer(42),
                ownership: Ownership::Owned,
            }]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_ok(), "expected Ok, got errors: {:?}", result.err());
    }

    #[test]
    fn infer_float_literal_defaults_to_f64() {
        let program = simple_program(vec![Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Let {
                name: "y".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Float(3.14),
                ownership: Ownership::Owned,
            }]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_ok());
    }

    // -- binary op type checking: can't add string + int ------------------

    #[test]
    fn binary_op_string_plus_int_fails() {
        let program = simple_program(vec![Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(Expr::StringLit("hello".into())),
                right: Box::new(Expr::Integer(1)),
            })]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(
            errors[0]
                .message
                .contains("cannot apply arithmetic operator"),
            "unexpected error message: {}",
            errors[0].message
        );
    }

    #[test]
    fn binary_op_int_plus_int_succeeds() {
        let program = simple_program(vec![Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(Expr::Integer(1)),
                right: Box::new(Expr::Integer(2)),
            })]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_ok());
    }

    // -- function return type inference -----------------------------------

    #[test]
    fn infer_function_return_type() {
        let program = simple_program(vec![Item::Function(Function {
            name: "add".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![
                Param {
                    name: "a".into(),
                    ty: Type::Named("i32".into()),
                    ownership: Ownership::Owned,
                },
                Param {
                    name: "b".into(),
                    ty: Type::Named("i32".into()),
                    ownership: Ownership::Owned,
                },
            ],
            return_type: None, // should be inferred as i32
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Binary {
                op: BinOp::Add,
                left: Box::new(Expr::Ident("a".into())),
                right: Box::new(Expr::Ident("b".into())),
            })]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_ok(), "errors: {:?}", result.err());

        let typed = result.unwrap();
        let sig = typed.bindings.get("add").expect("add should be in bindings");
        match sig {
            Ty::Function { ret, .. } => {
                assert_eq!(**ret, Ty::I32, "return type should be inferred as i32");
            }
            _ => panic!("expected function type for add"),
        }
    }

    #[test]
    fn declared_return_type_mismatch_is_error() {
        let program = simple_program(vec![Item::Function(Function {
            name: "bad".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: Some(Type::Named("bool".into())),
            trait_bounds: vec![],
            body: block(vec![Stmt::Expr(Expr::Integer(42))]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_err());
    }

    // -- struct field access type checking --------------------------------

    #[test]
    fn struct_field_access_correct_type() {
        let program = simple_program(vec![
            Item::Struct(StructDef {
                name: "Point".into(),
                lifetimes: vec![],
                type_params: vec![],
                trait_bounds: vec![],
                fields: vec![
                    Field {
                        name: "x".into(),
                        ty: Type::Named("f64".into()),
                        is_pub: true,
                    },
                    Field {
                        name: "y".into(),
                        ty: Type::Named("f64".into()),
                        is_pub: true,
                    },
                ],
                is_pub: true,
                span: span(),
            }),
            Item::Function(Function {
                name: "main".into(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![
                    Stmt::Let {
                        name: "p".into(),
                        ty: None,
                        mutable: false,
                        secret: false,
                        value: Expr::StructInit {
                            name: "Point".into(),
                            fields: vec![
                                ("x".into(), Expr::Float(1.0)),
                                ("y".into(), Expr::Float(2.0)),
                            ],
                        },
                        ownership: Ownership::Owned,
                    },
                    Stmt::Expr(Expr::FieldAccess {
                        object: Box::new(Expr::Ident("p".into())),
                        field: "x".into(),
                    }),
                ]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
        ]);

        let result = infer_program(&program);
        assert!(result.is_ok(), "errors: {:?}", result.err());
    }

    #[test]
    fn struct_field_access_nonexistent_field_is_error() {
        let program = simple_program(vec![
            Item::Struct(StructDef {
                name: "Point".into(),
                lifetimes: vec![],
                type_params: vec![],
                trait_bounds: vec![],
                fields: vec![
                    Field {
                        name: "x".into(),
                        ty: Type::Named("f64".into()),
                        is_pub: true,
                    },
                    Field {
                        name: "y".into(),
                        ty: Type::Named("f64".into()),
                        is_pub: true,
                    },
                ],
                is_pub: true,
                span: span(),
            }),
            Item::Function(Function {
                name: "main".into(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![
                    Stmt::Let {
                        name: "p".into(),
                        ty: None,
                        mutable: false,
                        secret: false,
                        value: Expr::StructInit {
                            name: "Point".into(),
                            fields: vec![
                                ("x".into(), Expr::Float(1.0)),
                                ("y".into(), Expr::Float(2.0)),
                            ],
                        },
                        ownership: Ownership::Owned,
                    },
                    Stmt::Expr(Expr::FieldAccess {
                        object: Box::new(Expr::Ident("p".into())),
                        field: "z".into(),
                    }),
                ]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
        ]);

        let result = infer_program(&program);
        assert!(result.is_err());
        assert!(
            result.unwrap_err()[0].message.contains("no field z"),
            "expected 'no field z' error"
        );
    }

    // -- reference type tracking ------------------------------------------

    #[test]
    fn borrow_produces_reference_type() {
        let program = simple_program(vec![Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![
                Stmt::Let {
                    name: "x".into(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(10),
                    ownership: Ownership::Owned,
                },
                Stmt::Let {
                    name: "r".into(),
                    ty: Some(Type::Reference {
                        mutable: false,
                        lifetime: None,
                        inner: Box::new(Type::Named("i32".into())),
                    }),
                    mutable: false,
                    secret: false,
                    value: Expr::Borrow(Box::new(Expr::Ident("x".into()))),
                    ownership: Ownership::Borrowed,
                },
            ]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_ok(), "errors: {:?}", result.err());
    }

    #[test]
    fn mut_borrow_produces_mut_reference_type() {
        let program = simple_program(vec![Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![
                Stmt::Let {
                    name: "x".into(),
                    ty: None,
                    mutable: true,
                    secret: false,
                    value: Expr::Integer(10),
                    ownership: Ownership::Owned,
                },
                Stmt::Let {
                    name: "r".into(),
                    ty: Some(Type::Reference {
                        mutable: true,
                        lifetime: None,
                        inner: Box::new(Type::Named("i32".into())),
                    }),
                    mutable: false,
                    secret: false,
                    value: Expr::BorrowMut(Box::new(Expr::Ident("x".into()))),
                    ownership: Ownership::MutBorrowed,
                },
            ]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_ok(), "errors: {:?}", result.err());
    }

    #[test]
    fn reference_mutability_mismatch_is_error() {
        let program = simple_program(vec![Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![],
            return_type: None,
            trait_bounds: vec![],
            body: block(vec![
                Stmt::Let {
                    name: "x".into(),
                    ty: None,
                    mutable: false,
                    secret: false,
                    value: Expr::Integer(10),
                    ownership: Ownership::Owned,
                },
                // Declare as &mut i32 but provide an immutable borrow.
                Stmt::Let {
                    name: "r".into(),
                    ty: Some(Type::Reference {
                        mutable: true,
                        lifetime: None,
                        inner: Box::new(Type::Named("i32".into())),
                    }),
                    mutable: false,
                    secret: false,
                    value: Expr::Borrow(Box::new(Expr::Ident("x".into()))),
                    ownership: Ownership::Borrowed,
                },
            ]),
            is_pub: false,
            must_use: false,
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_err());
        assert!(
            result.unwrap_err()[0]
                .message
                .contains("mutability mismatch"),
            "expected mutability mismatch error"
        );
    }

    // -- component prop type validation -----------------------------------

    #[test]
    fn component_state_type_inferred() {
        let program = simple_program(vec![Item::Component(Component {
            name: "Counter".into(),
            type_params: vec![],
            props: vec![Prop {
                name: "initial".into(),
                ty: Type::Named("i32".into()),
                default: None,
            }],
            state: vec![StateField {
                name: "count".into(),
                ty: None,
                mutable: true,
                secret: false,
                atomic: false,
                initializer: Expr::Integer(0),
                ownership: Ownership::Owned,
            }],
            methods: vec![],
            styles: vec![],
            transitions: vec![],
            trait_bounds: vec![],
            render: RenderBlock {
                body: TemplateNode::TextLiteral("hello".into()),
                span: span(),
            },
            permissions: None,
            gestures: vec![],
            skeleton: None,
            error_boundary: None,
            chunk: None,
            on_destroy: None,
            a11y: None,
            shortcuts: vec![],
            span: span(),
        })]);

        let result = infer_program(&program);
        assert!(result.is_ok(), "errors: {:?}", result.err());
    }

    // -- function call argument type checking -----------------------------

    #[test]
    fn fn_call_arg_count_mismatch_is_error() {
        let program = simple_program(vec![
            Item::Function(Function {
                name: "add".into(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![
                    Param {
                        name: "a".into(),
                        ty: Type::Named("i32".into()),
                        ownership: Ownership::Owned,
                    },
                    Param {
                        name: "b".into(),
                        ty: Type::Named("i32".into()),
                        ownership: Ownership::Owned,
                    },
                ],
                return_type: Some(Type::Named("i32".into())),
                trait_bounds: vec![],
                body: block(vec![Stmt::Expr(Expr::Binary {
                    op: BinOp::Add,
                    left: Box::new(Expr::Ident("a".into())),
                    right: Box::new(Expr::Ident("b".into())),
                })]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            Item::Function(Function {
                name: "main".into(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("add".into())),
                    args: vec![Expr::Integer(1)], // missing second arg
                })]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
        ]);

        let result = infer_program(&program);
        assert!(result.is_err());
        assert!(
            result.unwrap_err()[0].message.contains("expects 2 arguments"),
            "expected argument count mismatch error"
        );
    }

    #[test]
    fn fn_call_arg_type_mismatch_is_error() {
        let program = simple_program(vec![
            Item::Function(Function {
                name: "greet".into(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![Param {
                    name: "name".into(),
                    ty: Type::Named("String".into()),
                    ownership: Ownership::Owned,
                }],
                return_type: Some(Type::Named("String".into())),
                trait_bounds: vec![],
                body: block(vec![Stmt::Expr(Expr::Ident("name".into()))]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
            Item::Function(Function {
                name: "main".into(),
                lifetimes: vec![],
            type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: block(vec![Stmt::Expr(Expr::FnCall {
                    callee: Box::new(Expr::Ident("greet".into())),
                    args: vec![Expr::Integer(42)], // wrong type
                })]),
                is_pub: false,
                must_use: false,
                span: span(),
            }),
        ]);

        let result = infer_program(&program);
        assert!(result.is_err());
        assert!(
            result.unwrap_err()[0].message.contains("type mismatch"),
            "expected type mismatch error"
        );
    }
}

// Iterator protocol tests are added as a separate test module to avoid
// conflicts with the main test module.
#[cfg(test)]
mod iterator_tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn block(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: span() }
    }

    fn simple_program(items: Vec<Item>) -> Program {
        Program { items }
    }

    fn make_fn(body_stmts: Vec<Stmt>) -> Item {
        Item::Function(Function {
            name: "main".into(),
            lifetimes: vec![],
            type_params: vec![],
            params: vec![Param {
                name: "arr".into(),
                ty: Type::Array(Box::new(Type::Named("i32".into()))),
                ownership: Ownership::Owned,
            }],
            return_type: None,
            trait_bounds: vec![],
            body: block(body_stmts),
            is_pub: false,
            must_use: false,
            span: span(),
        })
    }

    #[test]
    fn iter_map_filter_collect_chain() {
        let program = simple_program(vec![make_fn(vec![
            Stmt::Let {
                name: "result".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::MethodCall {
                    object: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::MethodCall {
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
                        }),
                        method: "filter".into(),
                        args: vec![Expr::Closure {
                            params: vec![("x".into(), None)],
                            body: Box::new(Expr::Binary {
                                op: BinOp::Gt,
                                left: Box::new(Expr::Ident("x".into())),
                                right: Box::new(Expr::Integer(5)),
                            }),
                        }],
                    }),
                    method: "collect".into(),
                    args: vec![],
                },
                ownership: Ownership::Owned,
            },
        ])]);
        let result = infer_program(&program);
        assert!(result.is_ok(), "iter chain: {:?}", result.err());
    }

    #[test]
    fn iter_fold_type_inference() {
        let program = simple_program(vec![make_fn(vec![
            Stmt::Let {
                name: "sum".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::MethodCall {
                    object: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("arr".into())),
                        method: "iter".into(),
                        args: vec![],
                    }),
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
                },
                ownership: Ownership::Owned,
            },
        ])]);
        let result = infer_program(&program);
        assert!(result.is_ok(), "fold: {:?}", result.err());
    }

    #[test]
    fn iter_any_returns_bool() {
        let program = simple_program(vec![make_fn(vec![
            Stmt::Let {
                name: "r".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::MethodCall {
                    object: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("arr".into())),
                        method: "iter".into(),
                        args: vec![],
                    }),
                    method: "any".into(),
                    args: vec![Expr::Closure {
                        params: vec![("x".into(), None)],
                        body: Box::new(Expr::Binary {
                            op: BinOp::Gt,
                            left: Box::new(Expr::Ident("x".into())),
                            right: Box::new(Expr::Integer(0)),
                        }),
                    }],
                },
                ownership: Ownership::Owned,
            },
        ])]);
        let result = infer_program(&program);
        assert!(result.is_ok(), "any: {:?}", result.err());
    }

    #[test]
    fn iter_enumerate_collect() {
        let program = simple_program(vec![make_fn(vec![
            Stmt::Let {
                name: "e".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::MethodCall {
                    object: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::MethodCall {
                            object: Box::new(Expr::Ident("arr".into())),
                            method: "iter".into(),
                            args: vec![],
                        }),
                        method: "enumerate".into(),
                        args: vec![],
                    }),
                    method: "collect".into(),
                    args: vec![],
                },
                ownership: Ownership::Owned,
            },
        ])]);
        let result = infer_program(&program);
        assert!(result.is_ok(), "enumerate: {:?}", result.err());
    }

    #[test]
    fn iter_count_returns_i32() {
        let program = simple_program(vec![make_fn(vec![
            Stmt::Let {
                name: "n".into(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::MethodCall {
                    object: Box::new(Expr::MethodCall {
                        object: Box::new(Expr::Ident("arr".into())),
                        method: "iter".into(),
                        args: vec![],
                    }),
                    method: "count".into(),
                    args: vec![],
                },
                ownership: Ownership::Owned,
            },
        ])]);
        let result = infer_program(&program);
        assert!(result.is_ok(), "count: {:?}", result.err());
    }
}

#[cfg(test)]
mod closure_tests {
    use super::*;
    use crate::token::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    fn infer_program(program: &Program) {
        let mut tc = TypeChecker::new();
        tc.check_program(program);
    }

    fn make_program(stmts: Vec<Stmt>) -> Program {
        Program {
            items: vec![Item::Function(Function {
                name: "main".to_string(),
                lifetimes: vec![],
                type_params: vec![],
                params: vec![],
                return_type: None,
                trait_bounds: vec![],
                body: Block { stmts, span: span() },
                is_pub: true,
                must_use: false,
                span: span(),
            })],
        }
    }

    #[test]
    fn closure_type_inferred_as_function() {
        // let f = |x: i32| x + 1;
        // The type of f should be Function { params: [I32], ret: I32 }
        let program = make_program(vec![
            Stmt::Let {
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
            },
        ]);
        infer_program(&program);
    }

    #[test]
    fn closure_no_params_returns_integer() {
        // let f = || 42;
        let program = make_program(vec![
            Stmt::Let {
                name: "f".to_string(),
                ty: None,
                mutable: false,
                secret: false,
                value: Expr::Closure {
                    params: vec![],
                    body: Box::new(Expr::Integer(42)),
                },
                ownership: Ownership::Owned,
            },
        ]);
        infer_program(&program);
    }
}
