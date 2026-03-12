//! Nectar Standard Library
//!
//! Defines built-in types and functions available to every Nectar program.
//! The type checker and codegen consult this module to resolve standard
//! library names without requiring explicit imports.

use std::collections::HashMap;

use crate::ast::Type;

// ---------------------------------------------------------------------------
// Core data structures
// ---------------------------------------------------------------------------

/// A built-in type registered in the standard library.
#[derive(Debug, Clone)]
pub struct BuiltinType {
    /// The type name as it appears in Nectar source code (e.g. "Vec").
    pub name: String,
    /// Number of generic type parameters (e.g. Vec<T> = 1, HashMap<K,V> = 2).
    pub type_params: Vec<String>,
    /// Human-readable description.
    pub description: String,
    /// Methods available on this type.
    pub methods: Vec<BuiltinFn>,
    /// Variants (only meaningful for enum-like types such as Option/Result).
    pub variants: Vec<BuiltinVariant>,
}

/// A variant of a built-in enum type (e.g. Some(T), None).
#[derive(Debug, Clone)]
pub struct BuiltinVariant {
    pub name: String,
    pub fields: Vec<Type>,
}

/// A built-in function or method.
#[derive(Debug, Clone)]
pub struct BuiltinFn {
    /// Function or method name.
    pub name: String,
    /// Parameter types (excludes `self` for methods).
    pub params: Vec<BuiltinParam>,
    /// Return type.
    pub return_type: Type,
    /// Whether this is a method that takes `self`.
    pub takes_self: bool,
    /// Whether `self` is taken mutably.
    pub self_mutable: bool,
    /// Human-readable description.
    pub description: String,
}

/// A parameter in a built-in function signature.
#[derive(Debug, Clone)]
pub struct BuiltinParam {
    pub name: String,
    pub ty: Type,
}

// ---------------------------------------------------------------------------
// StdLib — the registry
// ---------------------------------------------------------------------------

/// Registry of all built-in types and free functions available in Nectar.
#[derive(Debug)]
pub struct StdLib {
    types: HashMap<String, BuiltinType>,
    functions: HashMap<String, BuiltinFn>,
}

impl StdLib {
    /// Create a new standard library instance with all builtins registered.
    pub fn new() -> Self {
        let mut stdlib = StdLib {
            types: HashMap::new(),
            functions: HashMap::new(),
        };

        stdlib.register_vec();
        stdlib.register_hashmap();
        stdlib.register_option();
        stdlib.register_result();
        stdlib.register_string();
        stdlib.register_iterator_trait();
        stdlib.register_math_functions();
        stdlib.register_formatting_functions();
        stdlib.register_web_api_functions();

        stdlib
    }

    /// Look up a built-in type by name (e.g. "Vec", "Option").
    pub fn lookup_type(&self, name: &str) -> Option<&BuiltinType> {
        self.types.get(name)
    }

    /// Look up a free function by name (e.g. "abs", "format").
    pub fn lookup_fn(&self, name: &str) -> Option<&BuiltinFn> {
        self.functions.get(name)
    }

    /// Look up a method on a given type (e.g. type_name="Vec", method_name="push").
    pub fn lookup_method(&self, type_name: &str, method_name: &str) -> Option<&BuiltinFn> {
        self.types
            .get(type_name)
            .and_then(|ty| ty.methods.iter().find(|m| m.name == method_name))
    }

    /// Return an iterator over all registered type names.
    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(|s| s.as_str())
    }

    /// Return an iterator over all registered free-function names.
    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(|s| s.as_str())
    }

    // -----------------------------------------------------------------------
    // Private registration helpers
    // -----------------------------------------------------------------------

    fn register_type(&mut self, ty: BuiltinType) {
        self.types.insert(ty.name.clone(), ty);
    }

    fn register_fn(&mut self, f: BuiltinFn) {
        self.functions.insert(f.name.clone(), f);
    }

    // -- Vec<T> -------------------------------------------------------------
    // WASM hint: Vec is backed by a (ptr, len, cap) triple in linear memory.
    // Growing requires `memory.grow` or a bump-allocator realloc. Elements are
    // stored contiguously starting at `ptr`.
    fn register_vec(&mut self) {
        let t = Type::Named("T".into());
        let methods = vec![
            BuiltinFn {
                name: "push".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: t.clone() }],
                return_type: Type::Named("Unit".into()),
                takes_self: true,
                self_mutable: true,
                description: "Append an element to the end of the vector.".into(),
            },
            BuiltinFn {
                name: "pop".into(),
                params: vec![],
                return_type: Type::Option(Box::new(t.clone())),
                takes_self: true,
                self_mutable: true,
                description: "Remove and return the last element, or None if empty.".into(),
            },
            BuiltinFn {
                name: "len".into(),
                params: vec![],
                return_type: Type::Named("Int".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return the number of elements.".into(),
            },
            BuiltinFn {
                name: "get".into(),
                params: vec![BuiltinParam { name: "index".into(), ty: Type::Named("Int".into()) }],
                return_type: Type::Option(Box::new(t.clone())),
                takes_self: true,
                self_mutable: false,
                description: "Return the element at `index`, or None if out of bounds.".into(),
            },
            // WASM hint: `iter` returns an i32 "iterator handle" that the
            // runtime tracks — successive `next` calls read elements from
            // linear memory via the handle's internal cursor.
            BuiltinFn {
                name: "iter".into(),
                params: vec![],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator over the elements.".into(),
            },
            BuiltinFn {
                name: "map".into(),
                params: vec![BuiltinParam {
                    name: "f".into(),
                    ty: Type::Function {
                        params: vec![t.clone()],
                        ret: Box::new(Type::Named("U".into())),
                    },
                }],
                return_type: Type::Named("Vec".into()),
                takes_self: true,
                self_mutable: false,
                description: "Apply `f` to each element and collect results into a new Vec.".into(),
            },
            BuiltinFn {
                name: "filter".into(),
                params: vec![BuiltinParam {
                    name: "predicate".into(),
                    ty: Type::Function {
                        params: vec![Type::Reference { mutable: false, lifetime: None, inner: Box::new(t.clone()) }],
                        ret: Box::new(Type::Named("Bool".into())),
                    },
                }],
                return_type: Type::Named("Vec".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return a new Vec containing only elements where `predicate` returns true.".into(),
            },
            BuiltinFn {
                name: "is_empty".into(),
                params: vec![],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if the vector contains no elements.".into(),
            },
        ];

        self.register_type(BuiltinType {
            name: "Vec".into(),
            type_params: vec!["T".into()],
            description: "A growable array backed by contiguous linear memory.".into(),
            methods,
            variants: vec![],
        });
    }

    // -- HashMap<K, V> ------------------------------------------------------
    // WASM hint: Implemented as an open-addressing hash table in linear memory.
    // Keys are hashed with a simple FNV-1a; buckets store (hash, key_ptr, val_ptr)
    // triples. Resize doubles the bucket array via realloc.
    fn register_hashmap(&mut self) {
        let k = Type::Named("K".into());
        let v = Type::Named("V".into());
        let methods = vec![
            BuiltinFn {
                name: "insert".into(),
                params: vec![
                    BuiltinParam { name: "key".into(), ty: k.clone() },
                    BuiltinParam { name: "value".into(), ty: v.clone() },
                ],
                return_type: Type::Option(Box::new(v.clone())),
                takes_self: true,
                self_mutable: true,
                description: "Insert a key-value pair. Returns the previous value if the key existed.".into(),
            },
            BuiltinFn {
                name: "get".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(k.clone()) } }],
                return_type: Type::Option(Box::new(Type::Reference { mutable: false, lifetime: None, inner: Box::new(v.clone()) })),
                takes_self: true,
                self_mutable: false,
                description: "Return a reference to the value for `key`, or None.".into(),
            },
            BuiltinFn {
                name: "remove".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(k.clone()) } }],
                return_type: Type::Option(Box::new(v.clone())),
                takes_self: true,
                self_mutable: true,
                description: "Remove and return the value for `key`, or None.".into(),
            },
            BuiltinFn {
                name: "contains_key".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(k.clone()) } }],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if the map contains the given key.".into(),
            },
            BuiltinFn {
                name: "keys".into(),
                params: vec![],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator over the keys.".into(),
            },
            BuiltinFn {
                name: "values".into(),
                params: vec![],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator over the values.".into(),
            },
            BuiltinFn {
                name: "len".into(),
                params: vec![],
                return_type: Type::Named("Int".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return the number of entries.".into(),
            },
        ];

        self.register_type(BuiltinType {
            name: "HashMap".into(),
            type_params: vec!["K".into(), "V".into()],
            description: "A hash map using open addressing in linear memory.".into(),
            methods,
            variants: vec![],
        });
    }

    // -- Option<T> ----------------------------------------------------------
    // WASM hint: Represented as a tagged union: byte 0 = discriminant
    // (0 = None, 1 = Some), followed by the payload for Some.
    fn register_option(&mut self) {
        let t = Type::Named("T".into());
        let methods = vec![
            BuiltinFn {
                name: "is_some".into(),
                params: vec![],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if this is Some.".into(),
            },
            BuiltinFn {
                name: "is_none".into(),
                params: vec![],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if this is None.".into(),
            },
            BuiltinFn {
                name: "unwrap".into(),
                params: vec![],
                return_type: t.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return the contained value. Traps if None.".into(),
            },
            BuiltinFn {
                name: "unwrap_or".into(),
                params: vec![BuiltinParam { name: "default".into(), ty: t.clone() }],
                return_type: t.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return the contained value or `default` if None.".into(),
            },
            BuiltinFn {
                name: "map".into(),
                params: vec![BuiltinParam {
                    name: "f".into(),
                    ty: Type::Function {
                        params: vec![t.clone()],
                        ret: Box::new(Type::Named("U".into())),
                    },
                }],
                return_type: Type::Option(Box::new(Type::Named("U".into()))),
                takes_self: true,
                self_mutable: false,
                description: "Apply `f` to the contained value if Some, returning Option<U>.".into(),
            },
        ];

        self.register_type(BuiltinType {
            name: "Option".into(),
            type_params: vec!["T".into()],
            description: "A value that is either Some(T) or None.".into(),
            methods,
            variants: vec![
                BuiltinVariant { name: "Some".into(), fields: vec![t.clone()] },
                BuiltinVariant { name: "None".into(), fields: vec![] },
            ],
        });
    }

    // -- Result<T, E> -------------------------------------------------------
    // WASM hint: Tagged union like Option — discriminant byte followed by
    // either the Ok payload or the Err payload.
    fn register_result(&mut self) {
        let t = Type::Named("T".into());
        let e = Type::Named("E".into());
        let methods = vec![
            BuiltinFn {
                name: "is_ok".into(),
                params: vec![],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if this is Ok.".into(),
            },
            BuiltinFn {
                name: "is_err".into(),
                params: vec![],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if this is Err.".into(),
            },
            BuiltinFn {
                name: "unwrap".into(),
                params: vec![],
                return_type: t.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return the Ok value. Traps if Err.".into(),
            },
            BuiltinFn {
                name: "map".into(),
                params: vec![BuiltinParam {
                    name: "f".into(),
                    ty: Type::Function {
                        params: vec![t.clone()],
                        ret: Box::new(Type::Named("U".into())),
                    },
                }],
                return_type: Type::Named("Result".into()),
                takes_self: true,
                self_mutable: false,
                description: "Apply `f` to the Ok value, leaving Err untouched.".into(),
            },
            BuiltinFn {
                name: "map_err".into(),
                params: vec![BuiltinParam {
                    name: "f".into(),
                    ty: Type::Function {
                        params: vec![e.clone()],
                        ret: Box::new(Type::Named("F".into())),
                    },
                }],
                return_type: Type::Named("Result".into()),
                takes_self: true,
                self_mutable: false,
                description: "Apply `f` to the Err value, leaving Ok untouched.".into(),
            },
        ];

        self.register_type(BuiltinType {
            name: "Result".into(),
            type_params: vec!["T".into(), "E".into()],
            description: "A value that is either Ok(T) or Err(E).".into(),
            methods,
            variants: vec![
                BuiltinVariant { name: "Ok".into(), fields: vec![t] },
                BuiltinVariant { name: "Err".into(), fields: vec![e] },
            ],
        });
    }

    // -- String -------------------------------------------------------------
    // WASM hint: Strings are stored as (ptr, len) pairs pointing to UTF-8
    // data in linear memory. The runtime maintains a string interning table
    // for literals. Concatenation allocates a new buffer.
    fn register_string(&mut self) {
        let string_ty = Type::Named("String".into());
        let methods = vec![
            BuiltinFn {
                name: "len".into(),
                params: vec![],
                return_type: Type::Named("Int".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return the byte length of the string.".into(),
            },
            BuiltinFn {
                name: "is_empty".into(),
                params: vec![],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if the string has zero length.".into(),
            },
            BuiltinFn {
                name: "contains".into(),
                params: vec![BuiltinParam { name: "pattern".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if the string contains the given pattern.".into(),
            },
            BuiltinFn {
                name: "starts_with".into(),
                params: vec![BuiltinParam { name: "prefix".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if the string starts with the given prefix.".into(),
            },
            BuiltinFn {
                name: "ends_with".into(),
                params: vec![BuiltinParam { name: "suffix".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return true if the string ends with the given suffix.".into(),
            },
            BuiltinFn {
                name: "trim".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return a new string with leading and trailing whitespace removed.".into(),
            },
            BuiltinFn {
                name: "split".into(),
                params: vec![BuiltinParam { name: "delimiter".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Named("Vec".into()),
                takes_self: true,
                self_mutable: false,
                description: "Split the string by `delimiter` and return a Vec<String>.".into(),
            },
            BuiltinFn {
                name: "to_uppercase".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return a new string with all characters converted to uppercase.".into(),
            },
            BuiltinFn {
                name: "to_lowercase".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return a new string with all characters converted to lowercase.".into(),
            },
            BuiltinFn {
                name: "push_str".into(),
                params: vec![BuiltinParam { name: "other".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Named("Unit".into()),
                takes_self: true,
                self_mutable: true,
                description: "Append another string to the end of this string.".into(),
            },
            // WASM hint: `chars` returns an iterator handle; each `next` call
            // decodes one UTF-8 codepoint from the (ptr, len) data, advancing
            // an internal byte offset.
            BuiltinFn {
                name: "chars".into(),
                params: vec![],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator over the characters (codepoints).".into(),
            },
            BuiltinFn {
                name: "concat".into(),
                params: vec![BuiltinParam { name: "other".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: string_ty.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return a new string that is the concatenation of self and other.".into(),
            },
            BuiltinFn {
                name: "substring".into(),
                params: vec![
                    BuiltinParam { name: "start".into(), ty: Type::Named("Int".into()) },
                    BuiltinParam { name: "end".into(), ty: Type::Named("Int".into()) },
                ],
                return_type: string_ty.clone(),
                takes_self: true,
                self_mutable: false,
                description: "Return the substring from byte index `start` to `end` (exclusive).".into(),
            },
            BuiltinFn {
                name: "index_of".into(),
                params: vec![BuiltinParam { name: "pattern".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Option(Box::new(Type::Named("Int".into()))),
                takes_self: true,
                self_mutable: false,
                description: "Return the byte index of the first occurrence of `pattern`, or None.".into(),
            },
        ];

        self.register_type(BuiltinType {
            name: "String".into(),
            type_params: vec![],
            description: "A UTF-8 encoded, growable string stored in linear memory.".into(),
            methods,
            variants: vec![],
        });
    }

    // -- Iterator trait ------------------------------------------------------
    // WASM hint: Iterators are represented as a (vtable_ptr, state_ptr) pair.
    // The vtable contains a single `next` function pointer. Higher-order
    // methods (map, filter, etc.) wrap the source iterator in a new state
    // struct that chains the transformation.
    fn register_iterator_trait(&mut self) {
        let t = Type::Named("T".into());
        let methods = vec![
            BuiltinFn {
                name: "next".into(),
                params: vec![],
                return_type: Type::Option(Box::new(t.clone())),
                takes_self: true,
                self_mutable: true,
                description: "Advance the iterator and return the next value, or None.".into(),
            },
            BuiltinFn {
                name: "map".into(),
                params: vec![BuiltinParam {
                    name: "f".into(),
                    ty: Type::Function {
                        params: vec![t.clone()],
                        ret: Box::new(Type::Named("U".into())),
                    },
                }],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator that applies `f` to each element.".into(),
            },
            BuiltinFn {
                name: "filter".into(),
                params: vec![BuiltinParam {
                    name: "predicate".into(),
                    ty: Type::Function {
                        params: vec![Type::Reference { mutable: false, lifetime: None, inner: Box::new(t.clone()) }],
                        ret: Box::new(Type::Named("Bool".into())),
                    },
                }],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator that yields only elements where `predicate` is true.".into(),
            },
            BuiltinFn {
                name: "fold".into(),
                params: vec![
                    BuiltinParam { name: "init".into(), ty: Type::Named("U".into()) },
                    BuiltinParam {
                        name: "f".into(),
                        ty: Type::Function {
                            params: vec![Type::Named("U".into()), t.clone()],
                            ret: Box::new(Type::Named("U".into())),
                        },
                    },
                ],
                return_type: Type::Named("U".into()),
                takes_self: true,
                self_mutable: true,
                description: "Fold every element into an accumulator starting from `init`.".into(),
            },
            BuiltinFn {
                name: "collect".into(),
                params: vec![],
                return_type: Type::Named("Vec".into()),
                takes_self: true,
                self_mutable: true,
                description: "Consume the iterator and collect all elements into a Vec.".into(),
            },
            BuiltinFn {
                name: "enumerate".into(),
                params: vec![],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator of (index, element) tuples.".into(),
            },
            BuiltinFn {
                name: "zip".into(),
                params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("Iterator".into()) }],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Zip this iterator with another, yielding pairs.".into(),
            },
            BuiltinFn {
                name: "take".into(),
                params: vec![BuiltinParam { name: "n".into(), ty: Type::Named("Int".into()) }],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator that yields at most `n` elements.".into(),
            },
            BuiltinFn {
                name: "skip".into(),
                params: vec![BuiltinParam { name: "n".into(), ty: Type::Named("Int".into()) }],
                return_type: Type::Named("Iterator".into()),
                takes_self: true,
                self_mutable: false,
                description: "Return an iterator that skips the first `n` elements.".into(),
            },
            BuiltinFn {
                name: "count".into(),
                params: vec![],
                return_type: Type::Named("Int".into()),
                takes_self: true,
                self_mutable: true,
                description: "Consume the iterator and return the number of elements.".into(),
            },
            BuiltinFn {
                name: "any".into(),
                params: vec![BuiltinParam {
                    name: "predicate".into(),
                    ty: Type::Function {
                        params: vec![Type::Reference { mutable: false, lifetime: None, inner: Box::new(t.clone()) }],
                        ret: Box::new(Type::Named("Bool".into())),
                    },
                }],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: true,
                description: "Return true if any element satisfies the predicate.".into(),
            },
            BuiltinFn {
                name: "all".into(),
                params: vec![BuiltinParam {
                    name: "predicate".into(),
                    ty: Type::Function {
                        params: vec![Type::Reference { mutable: false, lifetime: None, inner: Box::new(t.clone()) }],
                        ret: Box::new(Type::Named("Bool".into())),
                    },
                }],
                return_type: Type::Named("Bool".into()),
                takes_self: true,
                self_mutable: true,
                description: "Return true if all elements satisfy the predicate.".into(),
            },
            BuiltinFn {
                name: "find".into(),
                params: vec![BuiltinParam {
                    name: "predicate".into(),
                    ty: Type::Function {
                        params: vec![Type::Reference { mutable: false, lifetime: None, inner: Box::new(t.clone()) }],
                        ret: Box::new(Type::Named("Bool".into())),
                    },
                }],
                return_type: Type::Option(Box::new(t.clone())),
                takes_self: true,
                self_mutable: true,
                description: "Return the first element satisfying the predicate, or None.".into(),
            },
        ];

        self.register_type(BuiltinType {
            name: "Iterator".into(),
            type_params: vec!["T".into()],
            description: "The Iterator trait — lazy sequences with chainable transformations.".into(),
            methods,
            variants: vec![],
        });
    }

    // -- Math functions -----------------------------------------------------
    // WASM hint: Integer math maps directly to wasm i64 instructions.
    // Float math uses f64 instructions (f64.abs, f64.sqrt, f64.floor, etc.).
    // `pow` for integers is implemented as a loop; for floats it uses a
    // host-imported `Math.pow` or a software implementation.
    fn register_math_functions(&mut self) {
        let int_ty = Type::Named("Int".into());
        let float_ty = Type::Named("Float".into());

        let math_fns = vec![
            // abs — works for both Int and Float (overloaded)
            BuiltinFn {
                name: "abs".into(),
                params: vec![BuiltinParam { name: "x".into(), ty: int_ty.clone() }],
                return_type: int_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Return the absolute value. Works for Int and Float.".into(),
            },
            BuiltinFn {
                name: "min".into(),
                params: vec![
                    BuiltinParam { name: "a".into(), ty: int_ty.clone() },
                    BuiltinParam { name: "b".into(), ty: int_ty.clone() },
                ],
                return_type: int_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Return the smaller of two values.".into(),
            },
            BuiltinFn {
                name: "max".into(),
                params: vec![
                    BuiltinParam { name: "a".into(), ty: int_ty.clone() },
                    BuiltinParam { name: "b".into(), ty: int_ty.clone() },
                ],
                return_type: int_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Return the larger of two values.".into(),
            },
            BuiltinFn {
                name: "clamp".into(),
                params: vec![
                    BuiltinParam { name: "x".into(), ty: int_ty.clone() },
                    BuiltinParam { name: "lo".into(), ty: int_ty.clone() },
                    BuiltinParam { name: "hi".into(), ty: int_ty.clone() },
                ],
                return_type: int_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Clamp `x` to the range [lo, hi].".into(),
            },
            // WASM hint: i64 pow is a loop; f64 pow imports Math.pow.
            BuiltinFn {
                name: "pow".into(),
                params: vec![
                    BuiltinParam { name: "base".into(), ty: float_ty.clone() },
                    BuiltinParam { name: "exp".into(), ty: float_ty.clone() },
                ],
                return_type: float_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Raise `base` to the power `exp`.".into(),
            },
            // WASM hint: maps to f64.sqrt
            BuiltinFn {
                name: "sqrt".into(),
                params: vec![BuiltinParam { name: "x".into(), ty: float_ty.clone() }],
                return_type: float_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Return the square root of `x`.".into(),
            },
            // WASM hint: maps to f64.floor
            BuiltinFn {
                name: "floor".into(),
                params: vec![BuiltinParam { name: "x".into(), ty: float_ty.clone() }],
                return_type: float_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Round `x` down to the nearest integer.".into(),
            },
            // WASM hint: maps to f64.ceil
            BuiltinFn {
                name: "ceil".into(),
                params: vec![BuiltinParam { name: "x".into(), ty: float_ty.clone() }],
                return_type: float_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Round `x` up to the nearest integer.".into(),
            },
            // WASM hint: maps to f64.nearest
            BuiltinFn {
                name: "round".into(),
                params: vec![BuiltinParam { name: "x".into(), ty: float_ty.clone() }],
                return_type: float_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Round `x` to the nearest integer (ties to even).".into(),
            },
        ];

        for f in math_fns {
            self.register_fn(f);
        }
    }

    // -- Formatting & I/O ---------------------------------------------------
    // WASM hint: `print` and `println` call an imported host function
    // (e.g. `env.print`) passing (ptr, len) of the UTF-8 data.
    // `format` allocates a new String in linear memory using the same
    // interpolation engine. `to_string` is dispatched via a vtable pointer
    // for trait objects or monomorphised at compile time.
    fn register_formatting_functions(&mut self) {
        let string_ty = Type::Named("String".into());

        let fmt_fns = vec![
            BuiltinFn {
                name: "format".into(),
                params: vec![
                    BuiltinParam { name: "template".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } },
                    // Additional args are variadic — represented as a single
                    // "args" slice in the type system for now.
                    BuiltinParam { name: "args".into(), ty: Type::Array(Box::new(Type::Named("Any".into()))) },
                ],
                return_type: string_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Interpolate `args` into `template` placeholders and return a new String.".into(),
            },
            BuiltinFn {
                name: "to_string".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: Type::Named("Any".into()) }],
                return_type: string_ty.clone(),
                takes_self: false,
                self_mutable: false,
                description: "Convert any value to its String representation.".into(),
            },
            BuiltinFn {
                name: "print".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Named("Unit".into()),
                takes_self: false,
                self_mutable: false,
                description: "Print a string to standard output (no trailing newline).".into(),
            },
            BuiltinFn {
                name: "println".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: Type::Reference { mutable: false, lifetime: None, inner: Box::new(string_ty.clone()) } }],
                return_type: Type::Named("Unit".into()),
                takes_self: false,
                self_mutable: false,
                description: "Print a string to standard output followed by a newline.".into(),
            },
        ];

        for f in fmt_fns {
            self.register_fn(f);
        }
    }

    // -- Web API bindings -----------------------------------------------------
    // These register the signatures for web-platform APIs that Nectar programs can
    // call directly. Codegen maps these names to WASM imports from the
    // `webapi` module in the JS runtime.
    fn register_web_api_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let unit_ty = Type::Named("Unit".into());
        let i32_ty = Type::Named("i32".into());
        let f64_ty = Type::Named("f64".into());

        let web_fns = vec![
            // --- Storage ---
            BuiltinFn {
                name: "localStorage_get".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get a value from localStorage by key.".into(),
            },
            BuiltinFn {
                name: "localStorage_set".into(),
                params: vec![
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "value".into(), ty: string_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set a key-value pair in localStorage.".into(),
            },
            BuiltinFn {
                name: "localStorage_remove".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Remove a key from localStorage.".into(),
            },
            BuiltinFn {
                name: "sessionStorage_get".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get a value from sessionStorage by key.".into(),
            },
            BuiltinFn {
                name: "sessionStorage_set".into(),
                params: vec![
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "value".into(), ty: string_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set a key-value pair in sessionStorage.".into(),
            },

            // --- Clipboard ---
            BuiltinFn {
                name: "clipboard_write".into(),
                params: vec![BuiltinParam { name: "text".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Write text to the system clipboard (async).".into(),
            },
            BuiltinFn {
                name: "clipboard_read".into(),
                params: vec![BuiltinParam { name: "callback_idx".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Read text from the system clipboard (async, calls back with result).".into(),
            },

            // --- Timers ---
            BuiltinFn {
                name: "set_timeout".into(),
                params: vec![
                    BuiltinParam { name: "callback_idx".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "delay_ms".into(), ty: i32_ty.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Schedule a callback after a delay in milliseconds. Returns a timer ID.".into(),
            },
            BuiltinFn {
                name: "set_interval".into(),
                params: vec![
                    BuiltinParam { name: "callback_idx".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "interval_ms".into(), ty: i32_ty.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Schedule a repeating callback at an interval in milliseconds. Returns a timer ID.".into(),
            },
            BuiltinFn {
                name: "clear_timer".into(),
                params: vec![BuiltinParam { name: "timer_id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Cancel a timer created by set_timeout or set_interval.".into(),
            },

            // --- URL / History ---
            BuiltinFn {
                name: "get_location_href".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the current page URL (location.href).".into(),
            },
            BuiltinFn {
                name: "get_location_search".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the current URL query string (location.search).".into(),
            },
            BuiltinFn {
                name: "get_location_hash".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the current URL hash fragment (location.hash).".into(),
            },
            BuiltinFn {
                name: "push_state".into(),
                params: vec![BuiltinParam { name: "url".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Push a new URL to the browser history (history.pushState).".into(),
            },
            BuiltinFn {
                name: "replace_state".into(),
                params: vec![BuiltinParam { name: "url".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Replace the current URL in browser history (history.replaceState).".into(),
            },

            // --- Console ---
            BuiltinFn {
                name: "console_log".into(),
                params: vec![BuiltinParam { name: "msg".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Log a message to the browser console.".into(),
            },
            BuiltinFn {
                name: "console_warn".into(),
                params: vec![BuiltinParam { name: "msg".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Log a warning to the browser console.".into(),
            },
            BuiltinFn {
                name: "console_error".into(),
                params: vec![BuiltinParam { name: "msg".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Log an error to the browser console.".into(),
            },

            // --- Misc ---
            BuiltinFn {
                name: "random_float".into(),
                params: vec![],
                return_type: f64_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Generate a cryptographically-secure random float in [0, 1).".into(),
            },
            BuiltinFn {
                name: "performance_now".into(),
                params: vec![],
                return_type: f64_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "High-resolution timestamp from performance.now().".into(),
            },
            BuiltinFn {
                name: "request_animation_frame".into(),
                params: vec![BuiltinParam { name: "callback_idx".into(), ty: i32_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Schedule a callback for the next animation frame.".into(),
            },
        ];

        for f in web_fns {
            self.register_fn(f);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn stdlib() -> StdLib {
        StdLib::new()
    }

    // -- Type registration --------------------------------------------------

    #[test]
    fn all_standard_types_are_registered() {
        let lib = stdlib();
        let expected = ["Vec", "HashMap", "Option", "Result", "String", "Iterator"];
        for name in &expected {
            assert!(
                lib.lookup_type(name).is_some(),
                "expected built-in type `{}` to be registered",
                name,
            );
        }
    }

    #[test]
    fn type_param_counts_are_correct() {
        let lib = stdlib();
        assert_eq!(lib.lookup_type("Vec").unwrap().type_params.len(), 1);
        assert_eq!(lib.lookup_type("HashMap").unwrap().type_params.len(), 2);
        assert_eq!(lib.lookup_type("Option").unwrap().type_params.len(), 1);
        assert_eq!(lib.lookup_type("Result").unwrap().type_params.len(), 2);
        assert_eq!(lib.lookup_type("String").unwrap().type_params.len(), 0);
        assert_eq!(lib.lookup_type("Iterator").unwrap().type_params.len(), 1);
    }

    #[test]
    fn option_has_variants() {
        let lib = stdlib();
        let option = lib.lookup_type("Option").unwrap();
        let names: Vec<&str> = option.variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"Some"));
        assert!(names.contains(&"None"));
    }

    #[test]
    fn result_has_variants() {
        let lib = stdlib();
        let result = lib.lookup_type("Result").unwrap();
        let names: Vec<&str> = result.variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"Ok"));
        assert!(names.contains(&"Err"));
    }

    // -- Method lookup ------------------------------------------------------

    #[test]
    fn vec_push_has_correct_signature() {
        let lib = stdlib();
        let push = lib.lookup_method("Vec", "push")
            .expect("Vec.push should exist");
        assert!(push.takes_self);
        assert!(push.self_mutable);
        assert_eq!(push.params.len(), 1);
        assert_eq!(push.params[0].name, "value");
        match &push.return_type {
            Type::Named(n) => assert_eq!(n, "Unit"),
            other => panic!("expected Unit return type, got {:?}", other),
        }
    }

    #[test]
    fn vec_pop_returns_option() {
        let lib = stdlib();
        let pop = lib.lookup_method("Vec", "pop")
            .expect("Vec.pop should exist");
        assert!(matches!(pop.return_type, Type::Option(_)));
    }

    #[test]
    fn hashmap_insert_returns_option() {
        let lib = stdlib();
        let insert = lib.lookup_method("HashMap", "insert")
            .expect("HashMap.insert should exist");
        assert!(insert.self_mutable);
        assert!(matches!(insert.return_type, Type::Option(_)));
        assert_eq!(insert.params.len(), 2);
    }

    #[test]
    fn string_methods_are_complete() {
        let lib = stdlib();
        let expected_methods = [
            "len", "is_empty", "contains", "starts_with", "ends_with",
            "trim", "split", "to_uppercase", "to_lowercase", "push_str",
            "chars", "concat", "substring", "index_of",
        ];
        for method in &expected_methods {
            assert!(
                lib.lookup_method("String", method).is_some(),
                "expected String.{} to exist",
                method,
            );
        }
    }

    #[test]
    fn option_unwrap_or_takes_default() {
        let lib = stdlib();
        let unwrap_or = lib.lookup_method("Option", "unwrap_or")
            .expect("Option.unwrap_or should exist");
        assert_eq!(unwrap_or.params.len(), 1);
        assert_eq!(unwrap_or.params[0].name, "default");
    }

    #[test]
    fn iterator_methods_are_complete() {
        let lib = stdlib();
        let expected = [
            "next", "map", "filter", "fold", "collect", "enumerate",
            "zip", "take", "skip", "count", "any", "all", "find",
        ];
        for method in &expected {
            assert!(
                lib.lookup_method("Iterator", method).is_some(),
                "expected Iterator.{} to exist",
                method,
            );
        }
    }

    // -- Free-function lookup -----------------------------------------------

    #[test]
    fn math_functions_are_registered() {
        let lib = stdlib();
        let expected = ["abs", "min", "max", "clamp", "pow", "sqrt", "floor", "ceil", "round"];
        for name in &expected {
            assert!(
                lib.lookup_fn(name).is_some(),
                "expected math function `{}` to be registered",
                name,
            );
        }
    }

    #[test]
    fn format_returns_string() {
        let lib = stdlib();
        let fmt = lib.lookup_fn("format").expect("format should exist");
        match &fmt.return_type {
            Type::Named(n) => assert_eq!(n, "String"),
            other => panic!("expected String return type, got {:?}", other),
        }
    }

    #[test]
    fn print_and_println_are_registered() {
        let lib = stdlib();
        assert!(lib.lookup_fn("print").is_some());
        assert!(lib.lookup_fn("println").is_some());
    }

    #[test]
    fn to_string_is_registered() {
        let lib = stdlib();
        let ts = lib.lookup_fn("to_string").expect("to_string should exist");
        match &ts.return_type {
            Type::Named(n) => assert_eq!(n, "String"),
            other => panic!("expected String return type, got {:?}", other),
        }
    }

    #[test]
    fn nonexistent_lookups_return_none() {
        let lib = stdlib();
        assert!(lib.lookup_type("FooBar").is_none());
        assert!(lib.lookup_fn("does_not_exist").is_none());
        assert!(lib.lookup_method("Vec", "nonexistent_method").is_none());
        assert!(lib.lookup_method("NoSuchType", "anything").is_none());
    }
}
