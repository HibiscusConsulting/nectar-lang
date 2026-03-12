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
        stdlib.register_crypto_functions();
        stdlib.register_bigdecimal_type();
        stdlib.register_collections_functions();
        stdlib.register_url_functions();
        stdlib.register_mask_functions();
        stdlib.register_search_functions();
        stdlib.register_theme_functions();
        stdlib.register_auth_functions();
        stdlib.register_upload_functions();
        stdlib.register_db_functions();
        stdlib.register_animate_functions();
        stdlib.register_responsive_functions();
        stdlib.register_toast_functions();
        stdlib.register_data_table_type();
        stdlib.register_datepicker_functions();
        stdlib.register_debounce_throttle_functions();
        stdlib.register_skeleton_functions();
        stdlib.register_pagination_functions();
        stdlib.register_combobox_functions();
        stdlib.register_chart_functions();
        stdlib.register_editor_functions();
        stdlib.register_image_functions();
        stdlib.register_csv_functions();
        stdlib.register_maps_functions();
        stdlib.register_syntax_functions();
        stdlib.register_media_functions();
        stdlib.register_qr_functions();
        stdlib.register_share_functions();
        stdlib.register_wizard_functions();

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

    fn register_crypto_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let bool_ty = Type::Named("bool".into());
        let i32_ty = Type::Named("i32".into());
        let bytes_ty = Type::Array(Box::new(Type::Named("u8".into())));

        let crypto_fns = vec![
            BuiltinFn {
                name: "crypto_sha256".into(),
                params: vec![BuiltinParam { name: "data".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "SHA-256 hash of input data, returned as hex string. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_sha512".into(),
                params: vec![BuiltinParam { name: "data".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "SHA-512 hash of input data, returned as hex string. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_hmac".into(),
                params: vec![
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "data".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "HMAC-SHA256 of data using key. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_encrypt".into(),
                params: vec![
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "plaintext".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "AES-256-GCM encryption. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_decrypt".into(),
                params: vec![
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "ciphertext".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "AES-256-GCM decryption. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_sign".into(),
                params: vec![
                    BuiltinParam { name: "private_key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "data".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Ed25519 digital signature. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_verify".into(),
                params: vec![
                    BuiltinParam { name: "public_key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "data".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "signature".into(), ty: string_ty.clone() },
                ],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Ed25519 signature verification. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_derive_key".into(),
                params: vec![
                    BuiltinParam { name: "password".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "salt".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "PBKDF2 key derivation. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_random_uuid".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Generate a cryptographically random UUID v4. Pure WASM implementation.".into(),
            },
            BuiltinFn {
                name: "crypto_random_bytes".into(),
                params: vec![BuiltinParam { name: "length".into(), ty: i32_ty.clone() }],
                return_type: bytes_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Generate cryptographically random bytes. Pure WASM implementation.".into(),
            },
        ];

        for f in crypto_fns {
            self.register_fn(f);
        }
    }

    fn register_bigdecimal_type(&mut self) {
        let string_ty = Type::Named("String".into());
        let i64_ty = Type::Named("i64".into());
        let f64_ty = Type::Named("f64".into());
        let bool_ty = Type::Named("bool".into());
        let i32_ty = Type::Named("i32".into());

        self.register_type(BuiltinType {
            name: "BigDecimal".into(),
            type_params: vec![],
            description: "Arbitrary-precision decimal type. No floating-point errors. Pure WASM implementation.".into(),
            methods: vec![
                BuiltinFn {
                    name: "add".into(),
                    params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("BigDecimal".into()) }],
                    return_type: Type::Named("BigDecimal".into()),
                    takes_self: true, self_mutable: false,
                    description: "Add two BigDecimals.".into(),
                },
                BuiltinFn {
                    name: "sub".into(),
                    params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("BigDecimal".into()) }],
                    return_type: Type::Named("BigDecimal".into()),
                    takes_self: true, self_mutable: false,
                    description: "Subtract two BigDecimals.".into(),
                },
                BuiltinFn {
                    name: "mul".into(),
                    params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("BigDecimal".into()) }],
                    return_type: Type::Named("BigDecimal".into()),
                    takes_self: true, self_mutable: false,
                    description: "Multiply two BigDecimals.".into(),
                },
                BuiltinFn {
                    name: "div".into(),
                    params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("BigDecimal".into()) }],
                    return_type: Type::Named("BigDecimal".into()),
                    takes_self: true, self_mutable: false,
                    description: "Divide two BigDecimals.".into(),
                },
                BuiltinFn {
                    name: "eq".into(),
                    params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("BigDecimal".into()) }],
                    return_type: bool_ty.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Check equality.".into(),
                },
                BuiltinFn {
                    name: "gt".into(),
                    params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("BigDecimal".into()) }],
                    return_type: bool_ty.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Greater than comparison.".into(),
                },
                BuiltinFn {
                    name: "lt".into(),
                    params: vec![BuiltinParam { name: "other".into(), ty: Type::Named("BigDecimal".into()) }],
                    return_type: bool_ty.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Less than comparison.".into(),
                },
                BuiltinFn {
                    name: "to_string".into(),
                    params: vec![],
                    return_type: string_ty.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Convert to string representation.".into(),
                },
                BuiltinFn {
                    name: "to_fixed".into(),
                    params: vec![BuiltinParam { name: "digits".into(), ty: i32_ty.clone() }],
                    return_type: string_ty.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Format to fixed decimal places.".into(),
                },
            ],
            variants: vec![],
        });

        // Static constructors
        let constructors = vec![
            BuiltinFn {
                name: "BigDecimal_new".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: string_ty.clone() }],
                return_type: Type::Named("BigDecimal".into()),
                takes_self: false, self_mutable: false,
                description: "Create BigDecimal from string (e.g. \"19.99\").".into(),
            },
            BuiltinFn {
                name: "BigDecimal_from_i64".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: i64_ty.clone() }],
                return_type: Type::Named("BigDecimal".into()),
                takes_self: false, self_mutable: false,
                description: "Create BigDecimal from integer.".into(),
            },
            BuiltinFn {
                name: "BigDecimal_from_f64".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: f64_ty.clone() }],
                return_type: Type::Named("BigDecimal".into()),
                takes_self: false, self_mutable: false,
                description: "Create BigDecimal from float.".into(),
            },
        ];
        for f in constructors { self.register_fn(f); }
    }

    fn register_collections_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let any_array = Type::Array(Box::new(Type::Named("Any".into())));
        let bool_ty = Type::Named("bool".into());

        let fns = vec![
            BuiltinFn {
                name: "collections_group_by".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                ],
                return_type: Type::Named("HashMap".into()),
                takes_self: false, self_mutable: false,
                description: "Group array items by a key field. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "collections_sort_by".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                ],
                return_type: any_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Sort array items by a key field. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "collections_uniq_by".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                ],
                return_type: any_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Remove duplicates by key. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "collections_chunk".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "size".into(), ty: i32_ty.clone() },
                ],
                return_type: Type::Array(Box::new(any_array.clone())),
                takes_self: false, self_mutable: false,
                description: "Split array into chunks of given size. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "collections_flatten".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: Type::Array(Box::new(any_array.clone())) },
                ],
                return_type: any_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Flatten nested array one level. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "collections_zip".into(),
                params: vec![
                    BuiltinParam { name: "a".into(), ty: any_array.clone() },
                    BuiltinParam { name: "b".into(), ty: any_array.clone() },
                ],
                return_type: any_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Zip two arrays into array of pairs. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "collections_partition".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "predicate".into(), ty: Type::Named("Fn".into()) },
                ],
                return_type: Type::Tuple(vec![any_array.clone(), any_array.clone()]),
                takes_self: false, self_mutable: false,
                description: "Split array into two based on predicate. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_url_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let option_string = Type::Named("Option".into()); // Option<String>
        let bool_ty = Type::Named("bool".into());

        // Register Url struct type
        self.register_type(BuiltinType {
            name: "Url".into(),
            type_params: vec![],
            description: "Parsed URL with components. Pure WASM URL parser.".into(),
            methods: vec![],
            variants: vec![],
        });

        let fns = vec![
            BuiltinFn {
                name: "url_parse".into(),
                params: vec![BuiltinParam { name: "url".into(), ty: string_ty.clone() }],
                return_type: Type::Named("Url".into()),
                takes_self: false, self_mutable: false,
                description: "Parse URL string into Url struct. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "url_build".into(),
                params: vec![BuiltinParam { name: "base".into(), ty: string_ty.clone() }],
                return_type: Type::Named("Url".into()),
                takes_self: false, self_mutable: false,
                description: "Create URL builder from base. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "url_query_get".into(),
                params: vec![
                    BuiltinParam { name: "url".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                ],
                return_type: option_string.clone(),
                takes_self: false, self_mutable: false,
                description: "Get query parameter value by key. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "url_query_set".into(),
                params: vec![
                    BuiltinParam { name: "url".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "value".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set query parameter, returns new URL string. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_mask_functions(&mut self) {
        let string_ty = Type::Named("String".into());

        let fns = vec![
            BuiltinFn {
                name: "mask_phone".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Format as phone number (555) 123-4567. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "mask_currency".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Format as currency with commas and decimals. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "mask_credit_card".into(),
                params: vec![BuiltinParam { name: "value".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Format as credit card XXXX XXXX XXXX XXXX. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "mask_pattern".into(),
                params: vec![
                    BuiltinParam { name: "value".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "pattern".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Apply custom mask pattern (# = digit, A = letter, * = any). Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_search_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let any_array = Type::Array(Box::new(Type::Named("Any".into())));
        let string_array = Type::Array(Box::new(string_ty.clone()));

        self.register_type(BuiltinType {
            name: "SearchIndex".into(),
            type_params: vec![],
            description: "Client-side fuzzy search index. Pure WASM implementation.".into(),
            methods: vec![
                BuiltinFn {
                    name: "search".into(),
                    params: vec![BuiltinParam { name: "query".into(), ty: string_ty.clone() }],
                    return_type: any_array.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Search the index with a query string.".into(),
                },
            ],
            variants: vec![],
        });

        let fns = vec![
            BuiltinFn {
                name: "search_create_index".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "keys".into(), ty: string_array.clone() },
                ],
                return_type: Type::Named("SearchIndex".into()),
                takes_self: false, self_mutable: false,
                description: "Create a fuzzy search index over items using specified keys. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "search_query".into(),
                params: vec![
                    BuiltinParam { name: "index".into(), ty: Type::Named("SearchIndex".into()) },
                    BuiltinParam { name: "query".into(), ty: string_ty.clone() },
                ],
                return_type: any_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Search index with query, returns ranked results. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_theme_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let unit_ty = Type::Named("unit".into());

        let fns = vec![
            BuiltinFn {
                name: "theme_init".into(),
                params: vec![BuiltinParam { name: "config".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Initialize the theme system with a configuration. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "theme_toggle".into(),
                params: vec![],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Toggle between light and dark themes. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "theme_set".into(),
                params: vec![BuiltinParam { name: "name".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set the active theme by name. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "theme_current".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the name of the currently active theme. JS runtime bridge.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_auth_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let bool_ty = Type::Named("bool".into());
        let unit_ty = Type::Named("unit".into());
        let any_ty = Type::Named("Any".into());

        let fns = vec![
            BuiltinFn {
                name: "auth_init".into(),
                params: vec![BuiltinParam { name: "config".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Initialize the auth system with provider configuration. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "auth_login".into(),
                params: vec![
                    BuiltinParam { name: "username".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "password".into(), ty: string_ty.clone() },
                ],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Authenticate a user with username and password. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "auth_logout".into(),
                params: vec![],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Log out the current user. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "auth_get_user".into(),
                params: vec![],
                return_type: any_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the currently authenticated user object. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "auth_is_authenticated".into(),
                params: vec![],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Check whether a user is currently authenticated. JS runtime bridge.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_upload_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let bool_ty = Type::Named("bool".into());
        let unit_ty = Type::Named("unit".into());

        let fns = vec![
            BuiltinFn {
                name: "upload_init".into(),
                params: vec![BuiltinParam { name: "config".into(), ty: string_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Initialize the upload system with endpoint configuration. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "upload_start".into(),
                params: vec![BuiltinParam { name: "file_ref".into(), ty: string_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Start uploading a file. Returns upload ID. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "upload_cancel".into(),
                params: vec![BuiltinParam { name: "upload_id".into(), ty: string_ty.clone() }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Cancel an in-progress upload by ID. JS runtime bridge.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_db_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let bool_ty = Type::Named("bool".into());
        let any_ty = Type::Named("Any".into());
        let any_array = Type::Array(Box::new(any_ty.clone()));

        let fns = vec![
            BuiltinFn {
                name: "db_open".into(),
                params: vec![BuiltinParam { name: "name".into(), ty: string_ty.clone() }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Open or create a client-side database. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "db_put".into(),
                params: vec![
                    BuiltinParam { name: "key".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "value".into(), ty: any_ty.clone() },
                ],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Store a value by key in the database. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "db_get".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: string_ty.clone() }],
                return_type: any_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Retrieve a value by key from the database. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "db_delete".into(),
                params: vec![BuiltinParam { name: "key".into(), ty: string_ty.clone() }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Delete a value by key from the database. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "db_query".into(),
                params: vec![BuiltinParam { name: "query".into(), ty: string_ty.clone() }],
                return_type: any_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Query the database with a filter expression. JS runtime bridge.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_animate_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let f64_ty = Type::Named("f64".into());
        let bool_ty = Type::Named("bool".into());
        let any_ty = Type::Named("Any".into());

        let fns = vec![
            BuiltinFn {
                name: "animate_spring".into(),
                params: vec![
                    BuiltinParam { name: "target".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "config".into(), ty: any_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Start a spring animation. Returns animation ID. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "animate_keyframes".into(),
                params: vec![
                    BuiltinParam { name: "target".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "keyframes".into(), ty: any_ty.clone() },
                    BuiltinParam { name: "duration".into(), ty: f64_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Start a keyframes animation. Returns animation ID. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "animate_stagger".into(),
                params: vec![
                    BuiltinParam { name: "targets".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "config".into(), ty: any_ty.clone() },
                    BuiltinParam { name: "delay".into(), ty: f64_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Start a staggered animation across multiple targets. Returns animation ID. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "animate_cancel".into(),
                params: vec![BuiltinParam { name: "animation_id".into(), ty: string_ty.clone() }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Cancel a running animation by ID. JS runtime bridge.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    fn register_responsive_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let f64_ty = Type::Named("f64".into());
        let unit_ty = Type::Named("unit".into());
        let any_ty = Type::Named("Any".into());

        let fns = vec![
            BuiltinFn {
                name: "responsive_register_breakpoints".into(),
                params: vec![BuiltinParam { name: "breakpoints".into(), ty: any_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Register named breakpoints for responsive design. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "responsive_get_breakpoint".into(),
                params: vec![],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the name of the currently active breakpoint. JS runtime bridge.".into(),
            },
            BuiltinFn {
                name: "responsive_fluid".into(),
                params: vec![
                    BuiltinParam { name: "min_value".into(), ty: f64_ty.clone() },
                    BuiltinParam { name: "max_value".into(), ty: f64_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Generate a fluid CSS value that scales between min and max. JS runtime bridge.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Toast notifications ------------------------------------------------
    // Pure WASM — creates DOM elements via existing core DOM syscalls.
    fn register_toast_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let unit_ty = Type::Named("Unit".into());

        let fns = vec![
            BuiltinFn {
                name: "toast_success".into(),
                params: vec![BuiltinParam { name: "msg".into(), ty: string_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Show a success toast notification. Returns toast ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "toast_error".into(),
                params: vec![BuiltinParam { name: "msg".into(), ty: string_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Show an error toast notification. Returns toast ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "toast_warning".into(),
                params: vec![BuiltinParam { name: "msg".into(), ty: string_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Show a warning toast notification. Returns toast ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "toast_info".into(),
                params: vec![BuiltinParam { name: "msg".into(), ty: string_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Show an info toast notification. Returns toast ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "toast_dismiss".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Dismiss a specific toast by ID. Pure WASM — removes DOM element via syscalls.".into(),
            },
            BuiltinFn {
                name: "toast_dismiss_all".into(),
                params: vec![],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Dismiss all active toasts. Pure WASM — removes DOM elements via syscalls.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- DataTable<T> -------------------------------------------------------
    // Pure WASM computation for data table operations.
    fn register_data_table_type(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let t = Type::Named("T".into());
        let any_array = Type::Array(Box::new(Type::Named("Any".into())));
        let fn_ty = Type::Named("Fn".into());

        // Register Column type
        self.register_type(BuiltinType {
            name: "Column".into(),
            type_params: vec![],
            description: "Column definition for DataTable. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        self.register_type(BuiltinType {
            name: "DataTable".into(),
            type_params: vec!["T".into()],
            description: "A sortable, filterable, paginated data table. Pure WASM computation.".into(),
            methods: vec![
                BuiltinFn {
                    name: "sort".into(),
                    params: vec![
                        BuiltinParam { name: "column".into(), ty: string_ty.clone() },
                        BuiltinParam { name: "direction".into(), ty: string_ty.clone() },
                    ],
                    return_type: Type::Named("Unit".into()),
                    takes_self: true, self_mutable: true,
                    description: "Sort the table by a column. Pure WASM.".into(),
                },
                BuiltinFn {
                    name: "filter".into(),
                    params: vec![BuiltinParam { name: "predicate".into(), ty: fn_ty.clone() }],
                    return_type: Type::Named("Unit".into()),
                    takes_self: true, self_mutable: true,
                    description: "Filter rows by a predicate function. Pure WASM.".into(),
                },
                BuiltinFn {
                    name: "paginate".into(),
                    params: vec![
                        BuiltinParam { name: "page".into(), ty: i32_ty.clone() },
                        BuiltinParam { name: "per_page".into(), ty: i32_ty.clone() },
                    ],
                    return_type: Type::Named("Unit".into()),
                    takes_self: true, self_mutable: true,
                    description: "Set pagination parameters. Pure WASM.".into(),
                },
                BuiltinFn {
                    name: "pin_column".into(),
                    params: vec![BuiltinParam { name: "name".into(), ty: string_ty.clone() }],
                    return_type: Type::Named("Unit".into()),
                    takes_self: true, self_mutable: true,
                    description: "Pin a column so it stays visible during horizontal scroll. Pure WASM.".into(),
                },
                BuiltinFn {
                    name: "edit_cell".into(),
                    params: vec![
                        BuiltinParam { name: "row".into(), ty: i32_ty.clone() },
                        BuiltinParam { name: "column".into(), ty: string_ty.clone() },
                        BuiltinParam { name: "value".into(), ty: t.clone() },
                    ],
                    return_type: Type::Named("Unit".into()),
                    takes_self: true, self_mutable: true,
                    description: "Edit a cell value in-place. Pure WASM.".into(),
                },
                BuiltinFn {
                    name: "get_visible_rows".into(),
                    params: vec![],
                    return_type: any_array.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Get the currently visible (filtered, sorted, paginated) rows. Pure WASM.".into(),
                },
                BuiltinFn {
                    name: "export_csv".into(),
                    params: vec![],
                    return_type: string_ty.clone(),
                    takes_self: true, self_mutable: false,
                    description: "Export the table data as a CSV string. Pure WASM.".into(),
                },
            ],
            variants: vec![],
        });

        // DataTable constructor
        let fns = vec![
            BuiltinFn {
                name: "DataTable_new".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: any_array.clone() },
                    BuiltinParam { name: "columns".into(), ty: Type::Array(Box::new(Type::Named("Column".into()))) },
                ],
                return_type: Type::Named("DataTable".into()),
                takes_self: false, self_mutable: false,
                description: "Create a new DataTable with data and column definitions. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Datepicker ---------------------------------------------------------
    // Pure WASM — renders calendar via DOM syscalls.
    fn register_datepicker_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let unit_ty = Type::Named("Unit".into());

        // Register DatePickerOptions type
        self.register_type(BuiltinType {
            name: "DatePickerOptions".into(),
            type_params: vec![],
            description: "Options for creating a date picker. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        let fns = vec![
            BuiltinFn {
                name: "datepicker_create".into(),
                params: vec![BuiltinParam { name: "options".into(), ty: Type::Named("DatePickerOptions".into()) }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a date picker widget. Returns ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "datepicker_get_value".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the currently selected date as a string. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "datepicker_set_value".into(),
                params: vec![
                    BuiltinParam { name: "id".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "date".into(), ty: string_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set the selected date. Pure WASM — updates DOM via syscalls.".into(),
            },
            BuiltinFn {
                name: "datepicker_set_range".into(),
                params: vec![
                    BuiltinParam { name: "id".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "min".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "max".into(), ty: string_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set the allowed date range. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "datepicker_destroy".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy a date picker widget and remove its DOM elements. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Debounce / Throttle ------------------------------------------------
    // Pure WASM — uses timer syscall from core.
    fn register_debounce_throttle_functions(&mut self) {
        let i32_ty = Type::Named("i32".into());
        let fn_ty = Type::Named("Fn".into());

        let fns = vec![
            BuiltinFn {
                name: "debounce".into(),
                params: vec![
                    BuiltinParam { name: "callback".into(), ty: fn_ty.clone() },
                    BuiltinParam { name: "delay_ms".into(), ty: i32_ty.clone() },
                ],
                return_type: fn_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Debounce a callback — delays invocation until after delay_ms of inactivity. Pure WASM — uses timer syscall.".into(),
            },
            BuiltinFn {
                name: "throttle".into(),
                params: vec![
                    BuiltinParam { name: "callback".into(), ty: fn_ty.clone() },
                    BuiltinParam { name: "interval_ms".into(), ty: i32_ty.clone() },
                ],
                return_type: fn_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Throttle a callback — invokes at most once per interval_ms. Pure WASM — uses timer syscall.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Skeleton loaders ---------------------------------------------------
    // Pure WASM — creates shimmer placeholder elements via DOM syscalls.
    fn register_skeleton_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let unit_ty = Type::Named("Unit".into());

        let fns = vec![
            BuiltinFn {
                name: "skeleton_text".into(),
                params: vec![BuiltinParam { name: "lines".into(), ty: i32_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a text skeleton placeholder. Returns element ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "skeleton_circle".into(),
                params: vec![BuiltinParam { name: "size".into(), ty: i32_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a circular skeleton placeholder. Returns element ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "skeleton_rect".into(),
                params: vec![
                    BuiltinParam { name: "width".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "height".into(), ty: string_ty.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a rectangular skeleton placeholder. Returns element ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "skeleton_card".into(),
                params: vec![],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a card-shaped skeleton placeholder. Returns element ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "skeleton_avatar".into(),
                params: vec![BuiltinParam { name: "size".into(), ty: i32_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create an avatar skeleton placeholder. Returns element ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "skeleton_destroy".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy a skeleton placeholder and remove from DOM. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Pagination ---------------------------------------------------------
    // Pure WASM computation for paginating data.
    fn register_pagination_functions(&mut self) {
        let i32_ty = Type::Named("i32".into());
        let bool_ty = Type::Named("bool".into());
        let any_array = Type::Array(Box::new(Type::Named("Any".into())));
        let i32_array = Type::Array(Box::new(i32_ty.clone()));

        // Register Page<T> type
        self.register_type(BuiltinType {
            name: "Page".into(),
            type_params: vec!["T".into()],
            description: "A page of paginated results with metadata. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        let fns = vec![
            BuiltinFn {
                name: "pagination_paginate".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "page".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "per_page".into(), ty: i32_ty.clone() },
                ],
                return_type: Type::Named("Page".into()),
                takes_self: false, self_mutable: false,
                description: "Paginate an array of items. Returns a Page with items and metadata. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "pagination_page_numbers".into(),
                params: vec![
                    BuiltinParam { name: "current".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "total".into(), ty: i32_ty.clone() },
                ],
                return_type: i32_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Generate page number array for pagination UI. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "pagination_has_next".into(),
                params: vec![BuiltinParam { name: "page".into(), ty: Type::Named("Page".into()) }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Check if there is a next page. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "pagination_has_prev".into(),
                params: vec![BuiltinParam { name: "page".into(), ty: Type::Named("Page".into()) }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Check if there is a previous page. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Combobox -----------------------------------------------------------
    // Pure WASM — renders via DOM syscalls.
    fn register_combobox_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let unit_ty = Type::Named("Unit".into());
        let string_array = Type::Array(Box::new(string_ty.clone()));

        let fns = vec![
            BuiltinFn {
                name: "combobox_create".into(),
                params: vec![BuiltinParam { name: "options".into(), ty: string_array.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a combobox widget. Returns ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "combobox_get_selected".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: string_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Get selected items from the combobox. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "combobox_set_filter".into(),
                params: vec![
                    BuiltinParam { name: "id".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "query".into(), ty: string_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Apply a filter query to narrow the combobox options. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "combobox_destroy".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy a combobox widget and remove from DOM. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Chart --------------------------------------------------------------
    // Pure WASM — renders to SVG/Canvas via DOM syscalls.
    fn register_chart_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let f64_ty = Type::Named("f64".into());
        let unit_ty = Type::Named("Unit".into());
        let bool_ty = Type::Named("bool".into());

        // Register chart-related types
        self.register_type(BuiltinType {
            name: "Point".into(),
            type_params: vec![],
            description: "A 2D point with x and y coordinates. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        self.register_type(BuiltinType {
            name: "BarData".into(),
            type_params: vec![],
            description: "Data for a bar chart entry with label and value. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        self.register_type(BuiltinType {
            name: "PieSlice".into(),
            type_params: vec![],
            description: "Data for a pie chart slice with label, value, and color. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        self.register_type(BuiltinType {
            name: "ChartOptions".into(),
            type_params: vec![],
            description: "Options for chart rendering: width, height, title, animate. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        let point_array = Type::Array(Box::new(Type::Named("Point".into())));
        let bar_array = Type::Array(Box::new(Type::Named("BarData".into())));
        let pie_array = Type::Array(Box::new(Type::Named("PieSlice".into())));
        let chart_opts = Type::Named("ChartOptions".into());

        let fns = vec![
            BuiltinFn {
                name: "chart_line".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: point_array.clone() },
                    BuiltinParam { name: "options".into(), ty: chart_opts.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a line chart. Returns chart ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "chart_bar".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: bar_array.clone() },
                    BuiltinParam { name: "options".into(), ty: chart_opts.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a bar chart. Returns chart ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "chart_pie".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: pie_array.clone() },
                    BuiltinParam { name: "options".into(), ty: chart_opts.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a pie chart. Returns chart ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "chart_scatter".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: point_array.clone() },
                    BuiltinParam { name: "options".into(), ty: chart_opts.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a scatter chart. Returns chart ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "chart_update".into(),
                params: vec![
                    BuiltinParam { name: "id".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "data".into(), ty: point_array.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Update chart data. Pure WASM — re-renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "chart_destroy".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy a chart and remove from DOM. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Rich text editor ---------------------------------------------------
    // Pure WASM — manages contenteditable via DOM syscalls.
    fn register_editor_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let unit_ty = Type::Named("Unit".into());

        // Register EditorOptions type
        self.register_type(BuiltinType {
            name: "EditorOptions".into(),
            type_params: vec![],
            description: "Options for creating a rich text editor: mode, placeholder. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        let fns = vec![
            BuiltinFn {
                name: "editor_create".into(),
                params: vec![BuiltinParam { name: "options".into(), ty: Type::Named("EditorOptions".into()) }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a rich text editor. Returns editor ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "editor_get_content".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the editor content as HTML. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "editor_set_content".into(),
                params: vec![
                    BuiltinParam { name: "id".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "content".into(), ty: string_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set the editor content. Pure WASM — updates DOM via syscalls.".into(),
            },
            BuiltinFn {
                name: "editor_get_markdown".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the editor content as Markdown. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "editor_insert".into(),
                params: vec![
                    BuiltinParam { name: "id".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "text".into(), ty: string_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Insert text at the current cursor position. Pure WASM — updates DOM via syscalls.".into(),
            },
            BuiltinFn {
                name: "editor_destroy".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy an editor and remove from DOM. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Image manipulation -------------------------------------------------
    // Pure WASM pixel manipulation — no browser APIs needed.
    fn register_image_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let f32_ty = Type::Named("f32".into());
        let bytes_ty = Type::Array(Box::new(Type::Named("u8".into())));

        let fns = vec![
            BuiltinFn {
                name: "image_crop".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: bytes_ty.clone() },
                    BuiltinParam { name: "x".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "y".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "w".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "h".into(), ty: i32_ty.clone() },
                ],
                return_type: bytes_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Crop an image to the specified rectangle. Pure WASM pixel manipulation.".into(),
            },
            BuiltinFn {
                name: "image_resize".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: bytes_ty.clone() },
                    BuiltinParam { name: "width".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "height".into(), ty: i32_ty.clone() },
                ],
                return_type: bytes_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Resize an image to the specified dimensions. Pure WASM pixel manipulation.".into(),
            },
            BuiltinFn {
                name: "image_compress".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: bytes_ty.clone() },
                    BuiltinParam { name: "quality".into(), ty: f32_ty.clone() },
                ],
                return_type: bytes_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Compress an image with the given quality (0.0 to 1.0). Pure WASM.".into(),
            },
            BuiltinFn {
                name: "image_to_base64".into(),
                params: vec![BuiltinParam { name: "data".into(), ty: bytes_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Encode image data as a base64 string. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- CSV parsing and generation -----------------------------------------
    // Pure WASM string processing.
    fn register_csv_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let string_array = Type::Array(Box::new(string_ty.clone()));
        let row_array = Type::Array(Box::new(string_array.clone()));
        let any_array = Type::Array(Box::new(Type::Named("Any".into())));

        let fns = vec![
            BuiltinFn {
                name: "csv_parse".into(),
                params: vec![BuiltinParam { name: "input".into(), ty: string_ty.clone() }],
                return_type: row_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Parse a CSV string into a 2D array of strings. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "csv_stringify".into(),
                params: vec![BuiltinParam { name: "rows".into(), ty: row_array.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Convert a 2D array of strings into a CSV string. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "csv_parse_typed".into(),
                params: vec![BuiltinParam { name: "input".into(), ty: string_ty.clone() }],
                return_type: any_array.clone(),
                takes_self: false, self_mutable: false,
                description: "Parse a CSV string into typed objects (generic). Pure WASM.".into(),
            },
            BuiltinFn {
                name: "csv_export".into(),
                params: vec![
                    BuiltinParam { name: "items".into(), ty: any_array.clone() },
                    BuiltinParam { name: "columns".into(), ty: Type::Array(Box::new(string_ty.clone())) },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Export typed objects to CSV with specified columns. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Maps ---------------------------------------------------------------
    // Pure WASM — tile-based map rendering via DOM syscalls.
    fn register_maps_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let f64_ty = Type::Named("f64".into());
        let unit_ty = Type::Named("Unit".into());

        // Register MapOptions type
        self.register_type(BuiltinType {
            name: "MapOptions".into(),
            type_params: vec![],
            description: "Options for creating a map: center_lat, center_lng, zoom, tile_url. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        let fns = vec![
            BuiltinFn {
                name: "maps_create".into(),
                params: vec![
                    BuiltinParam { name: "container".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "options".into(), ty: Type::Named("MapOptions".into()) },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a map widget in the given container. Returns map ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "maps_add_marker".into(),
                params: vec![
                    BuiltinParam { name: "map".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "lat".into(), ty: f64_ty.clone() },
                    BuiltinParam { name: "lng".into(), ty: f64_ty.clone() },
                    BuiltinParam { name: "label".into(), ty: string_ty.clone() },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Add a marker to the map. Returns marker ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "maps_remove_marker".into(),
                params: vec![
                    BuiltinParam { name: "map".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "marker".into(), ty: i32_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Remove a marker from the map. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "maps_set_center".into(),
                params: vec![
                    BuiltinParam { name: "map".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "lat".into(), ty: f64_ty.clone() },
                    BuiltinParam { name: "lng".into(), ty: f64_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set the map center position. Pure WASM — updates DOM via syscalls.".into(),
            },
            BuiltinFn {
                name: "maps_set_zoom".into(),
                params: vec![
                    BuiltinParam { name: "map".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "level".into(), ty: i32_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Set the map zoom level. Pure WASM — updates DOM via syscalls.".into(),
            },
            BuiltinFn {
                name: "maps_destroy".into(),
                params: vec![BuiltinParam { name: "map".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy a map widget and remove from DOM. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Syntax highlighting ------------------------------------------------
    // Pure WASM — tokenizes code and wraps in span tags.
    fn register_syntax_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_array = Type::Array(Box::new(Type::Named("i32".into())));

        let fns = vec![
            BuiltinFn {
                name: "syntax_highlight".into(),
                params: vec![
                    BuiltinParam { name: "code".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "language".into(), ty: string_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Syntax-highlight code and return HTML with span class names. Pure WASM tokenizer.".into(),
            },
            BuiltinFn {
                name: "syntax_highlight_lines".into(),
                params: vec![
                    BuiltinParam { name: "code".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "language".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "lines".into(), ty: i32_array.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Syntax-highlight specific lines of code. Returns HTML string. Pure WASM tokenizer.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Media player -------------------------------------------------------
    // Pure WASM state management — uses DOM syscalls for video/audio elements.
    fn register_media_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let f64_ty = Type::Named("f64".into());
        let unit_ty = Type::Named("Unit".into());

        // Register MediaOptions type
        self.register_type(BuiltinType {
            name: "MediaOptions".into(),
            type_params: vec![],
            description: "Options for media player: controls, autoplay, loop_playback, captions_src. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        let fns = vec![
            BuiltinFn {
                name: "media_create_player".into(),
                params: vec![
                    BuiltinParam { name: "src".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "options".into(), ty: Type::Named("MediaOptions".into()) },
                ],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a media player for audio or video. Returns player ID. Pure WASM — renders via DOM syscalls.".into(),
            },
            BuiltinFn {
                name: "media_play".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Start playback. Pure WASM — invokes DOM syscall.".into(),
            },
            BuiltinFn {
                name: "media_pause".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Pause playback. Pure WASM — invokes DOM syscall.".into(),
            },
            BuiltinFn {
                name: "media_seek".into(),
                params: vec![
                    BuiltinParam { name: "id".into(), ty: i32_ty.clone() },
                    BuiltinParam { name: "time".into(), ty: f64_ty.clone() },
                ],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Seek to a specific time in seconds. Pure WASM — invokes DOM syscall.".into(),
            },
            BuiltinFn {
                name: "media_get_duration".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: f64_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the total duration of the media in seconds. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "media_get_current_time".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: f64_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the current playback time in seconds. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "media_destroy".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy a media player and remove from DOM. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- QR code generation -------------------------------------------------
    // Pure WASM — QR code algorithm runs in WASM, outputs SVG or pixel buffer.
    fn register_qr_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let bytes_ty = Type::Array(Box::new(Type::Named("u8".into())));

        let fns = vec![
            BuiltinFn {
                name: "qr_generate".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "size".into(), ty: i32_ty.clone() },
                ],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Generate a QR code as an SVG string. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "qr_generate_png".into(),
                params: vec![
                    BuiltinParam { name: "data".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "size".into(), ty: i32_ty.clone() },
                ],
                return_type: bytes_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Generate a QR code as a PNG pixel buffer. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Share --------------------------------------------------------------
    // WASM logic with one JS syscall (navigator.share).
    fn register_share_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let bool_ty = Type::Named("bool".into());

        let fns = vec![
            BuiltinFn {
                name: "share_native".into(),
                params: vec![
                    BuiltinParam { name: "title".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "text".into(), ty: string_ty.clone() },
                    BuiltinParam { name: "url".into(), ty: string_ty.clone() },
                ],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Trigger native share dialog (navigator.share). WASM logic + one JS syscall.".into(),
            },
            BuiltinFn {
                name: "share_can_share".into(),
                params: vec![],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Check if native sharing is available. WASM logic + one JS syscall.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
    }

    // -- Wizard (multi-step forms) ------------------------------------------
    // Pure WASM state machine.
    fn register_wizard_functions(&mut self) {
        let string_ty = Type::Named("String".into());
        let i32_ty = Type::Named("i32".into());
        let bool_ty = Type::Named("bool".into());
        let unit_ty = Type::Named("Unit".into());

        // Register WizardStep type
        self.register_type(BuiltinType {
            name: "WizardStep".into(),
            type_params: vec![],
            description: "A step in a wizard with name and validator. Pure WASM.".into(),
            methods: vec![],
            variants: vec![],
        });

        let step_array = Type::Array(Box::new(Type::Named("WizardStep".into())));

        let fns = vec![
            BuiltinFn {
                name: "wizard_create".into(),
                params: vec![BuiltinParam { name: "steps".into(), ty: step_array.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Create a multi-step wizard. Returns wizard ID. Pure WASM state machine.".into(),
            },
            BuiltinFn {
                name: "wizard_next".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Advance to the next step. Returns false if already at last step. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "wizard_prev".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Go back to the previous step. Returns false if already at first step. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "wizard_get_current_step".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: i32_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get the index of the current step. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "wizard_validate_step".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: bool_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Validate the current step using its validator function. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "wizard_get_data".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: string_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Get all wizard data as a JSON string. Pure WASM.".into(),
            },
            BuiltinFn {
                name: "wizard_destroy".into(),
                params: vec![BuiltinParam { name: "id".into(), ty: i32_ty.clone() }],
                return_type: unit_ty.clone(),
                takes_self: false, self_mutable: false,
                description: "Destroy a wizard and free its resources. Pure WASM.".into(),
            },
        ];
        for f in fns { self.register_fn(f); }
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
