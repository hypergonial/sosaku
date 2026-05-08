use std::{borrow::Cow, collections::HashMap, fmt::Debug};

use crate::{VTable, functions::DEFAULT_VTABLE, types::json::JsonValue};

macro_rules! define_env {
    () => {
        define_env!(@impl);
    };
    ($default:ty) => {
        define_env!(@impl = $default);
    };
    (@impl $(= $default:ty)?) => {
        /// The evaluation environment for a sosaku expression, containing variable bindings and a function vtable.
        ///
        /// To construct an Env, use `Env::new()` to create an [`EnvBuilder`], then call `build()`.
        #[derive(::std::fmt::Debug, ::std::clone::Clone)]
        pub struct Env<'var, 'vtable, V: crate::types::json::JsonValue + ::std::clone::Clone + ::std::fmt::Debug $(=$default)?> {
            bindings: ::std::collections::HashMap<::std::boxed::Box<::core::primitive::str>, ::std::borrow::Cow<'var, V>>,
            vtable: ::std::borrow::Cow<'vtable, crate::functions::VTable>,
        }
    };
}

#[cfg(feature = "serde_json")]
define_env!(::serde_json::Value);

#[cfg(not(feature = "serde_json"))]
define_env!();

impl<'var, 'vtable, V: JsonValue + Clone + Debug> Env<'var, 'vtable, V> {
    /// Create a new [`EnvBuilder`] for constructing an [`Env`].
    ///
    /// # Example
    /// ```rust
    ///  # #[cfg(feature = "serde_json")] {
    /// // Note: The `serde` feature must be enabled to use
    /// // `serde_json::Value` as the JSON value type.
    /// use sosaku::Env;
    /// let env = Env::<serde_json::Value>::new()
    ///     .bind("x", serde_json::json!(42))
    ///     .bind("y", serde_json::json!("hello"))
    ///     .build();
    ///
    /// assert_eq!(env.bindings().get("x").unwrap().as_ref(), &serde_json::json!(42));
    /// assert_eq!(env.bindings().get("y").unwrap().as_ref(), &serde_json::json!("hello"));
    /// # }
    /// ```
    #[expect(clippy::new_ret_no_self)]
    pub fn new() -> EnvBuilder<'var, 'vtable, V> {
        EnvBuilder::new()
    }

    /// Get a reference to the variable bindings in this environment.
    ///
    /// # Returns
    ///
    /// A reference to the variable bindings, which is a `HashMap` mapping variable names
    /// to their corresponding JSON values.
    #[inline]
    pub const fn bindings(&self) -> &HashMap<Box<str>, Cow<'var, V>> {
        &self.bindings
    }

    /// Get a mutable reference to the variable bindings in this environment.
    ///
    /// # Returns
    ///
    /// A mutable reference to the variable bindings, which is a `HashMap` mapping variable names
    /// to their corresponding JSON values. Modifying this will change the variable bindings in this environment
    /// and will affect any evaluations that use this environment after the modification.
    #[inline]
    pub const fn bindings_mut(&mut self) -> &mut HashMap<Box<str>, Cow<'var, V>> {
        &mut self.bindings
    }

    /// Get a reference to the active vtable.
    ///
    /// # Returns
    ///
    /// A reference to the `VTable` containing the function definitions available in this environment.
    #[inline]
    pub(crate) fn vtable(&self) -> &VTable {
        self.vtable.as_ref()
    }
}

/// A builder to construct an [`Env`].
///
/// # Example
/// ```rust
/// # #[cfg(feature = "serde_json")] {
/// // Note: The `serde` feature must be enabled to use
/// // `serde_json::Value` as the JSON value type.
/// use sosaku::Env;
/// let env = Env::<serde_json::Value>::new()
///     .bind("x", serde_json::json!(42))
///     .bind("y", serde_json::json!("hello"))
///     .build();
///
/// assert_eq!(env.bindings().get("x").unwrap().as_ref(), &serde_json::json!(42));
/// assert_eq!(env.bindings().get("y").unwrap().as_ref(), &serde_json::json!("hello"));
/// # }
/// ```
#[derive(Debug, Clone, Default)]
pub struct EnvBuilder<'var, 'vtable, V: JsonValue + Clone + Debug> {
    bindings: HashMap<Box<str>, Cow<'var, V>>,
    vtable: Option<Cow<'vtable, VTable>>,
}

impl<'var, 'vtable, V: JsonValue + Clone + Debug> EnvBuilder<'var, 'vtable, V> {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            vtable: None,
        }
    }

    /// Returns true if the given variable name is bound in this environment, false otherwise.
    pub fn is_bound(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Get a reference to the value bound to the given variable name, if it exists.
    ///
    /// # Parameters
    /// - `name`: The name of the variable to look up.
    ///
    /// # Returns
    ///
    /// - `Some(&Cow<'var, V>)` if the variable is bound in this environment,
    ///   where the `Cow` contains a reference to the value if it was bound using `bind_ref`,
    ///   or an owned value if it was bound using `bind`.
    /// - `None` if the variable is not bound in this environment.
    pub fn get_binding(&self, name: &str) -> Option<&Cow<'var, V>> {
        self.bindings.get(name)
    }

    /// Bind a variable name to a JSON value in this environment.
    /// If you want to bind a reference instead of an owned value, see [`EnvBuilder::bind_ref`].
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the variable to bind.
    /// - `value`: The JSON value to bind to the variable name.
    ///
    /// # Returns
    ///
    /// A mutable reference to this [`EnvBuilder`] for method chaining.
    pub fn bind(&mut self, name: impl Into<Box<str>>, value: impl Into<V>) -> &mut Self {
        self.bindings.insert(name.into(), Cow::Owned(value.into()));
        self
    }

    /// Bind multiple variable names to JSON values in this environment.
    /// If you want to bind references instead of owned values, see [`EnvBuilder::bind_ref_multiple`].
    ///
    /// # Parameters
    ///
    /// - `vars`: An iterable of `(name, value)` pairs, where `name` is the variable name to bind
    ///   and `value` is the JSON value to bind to that name.
    ///
    /// # Returns
    ///
    /// A mutable reference to this [`EnvBuilder`] for method chaining.
    pub fn bind_multiple(
        &mut self,
        vars: impl IntoIterator<Item = (impl Into<Box<str>>, impl Into<V>)>,
    ) -> &mut Self {
        for (name, value) in vars {
            self.bindings.insert(name.into(), Cow::Owned(value.into()));
        }
        self
    }

    /// Bind a reference to a JSON value in this environment, which allows the value
    /// to be shared across multiple environments without cloning. Additionally, when possible,
    /// the return value of an evaluation can be a reference to one of the bindings or literals,
    /// which can be more efficient than returning an owned value.
    /// If you want to bind an owned value instead of a reference, see [`EnvBuilder::bind`].
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the variable to bind.
    /// - `value`: A reference to the JSON value to bind to the variable name.
    ///
    /// # Returns
    ///
    /// A mutable reference to this [`EnvBuilder`] for method chaining.
    pub fn bind_ref(&mut self, name: impl Into<Box<str>>, value: impl Into<&'var V>) -> &mut Self {
        self.bindings
            .insert(name.into(), Cow::Borrowed(value.into()));
        self
    }

    /// Bind multiple references to JSON values in this environment, which allows the values
    /// to be shared across multiple environments without cloning. Additionally, when possible,
    /// the return value of an evaluation can be a reference to one of the bindings or literals,
    /// which can be more efficient than returning an owned value.
    /// If you want to bind owned values instead of references, see [`EnvBuilder::bind_multiple`].
    ///
    /// # Parameters
    ///
    /// - `vars`: An iterable of `(name, value)` pairs, where `name` is the variable name to bind
    ///   and `value` is a reference to the JSON value to bind to that name.
    ///
    /// # Returns
    ///
    /// A mutable reference to this [`EnvBuilder`] for method chaining.
    pub fn bind_ref_multiple(
        &mut self,
        vars: impl IntoIterator<Item = (impl Into<Box<str>>, impl Into<&'var V>)>,
    ) -> &mut Self {
        for (name, value) in vars {
            self.bindings
                .insert(name.into(), Cow::Borrowed(value.into()));
        }
        self
    }

    /// Use a custom vtable for this environment instead of the default one.
    /// This allows you to override the default function definitions or add new ones.
    ///
    /// Tip: You can create a custom vtable by cloning the default one and modifying it, e.g.:
    /// ```rust
    /// use sosaku::{Value, Env, VTable, DEFAULT_VTABLE, FnArgs, FnResult, FnCallback, FnCallError, EvalError};
    ///
    /// fn my_func(args: FnArgs<'_>) -> FnResult<'_> {
    ///     // Your function implementation goes here
    ///     if !args.is_empty() {
    ///         return Err(EvalError::ArgumentCount {
    ///             expected: 0,
    ///             got: args.len(),
    ///         });
    ///     }
    ///
    ///     Ok(Value::Int(42))
    /// }
    ///
    /// # #[cfg(feature = "serde_json")] {
    /// let mut custom_vtable = DEFAULT_VTABLE.clone();
    /// custom_vtable.insert("my_func", FnCallback::new_sync(my_func));
    /// let env = Env::<serde_json::Value>::new()
    ///    .bind("x", serde_json::json!(42))
    ///    .use_vtable(custom_vtable)
    ///    .build();
    /// # }
    /// ```
    ///
    /// # Parameters
    ///
    /// - `vtable`: The custom vtable to use for this environment.
    ///
    /// # Returns
    ///
    /// A mutable reference to this [`EnvBuilder`] for method chaining.
    pub fn use_vtable(&mut self, vtable: VTable) -> &mut Self {
        self.vtable = Some(Cow::Owned(vtable));
        self
    }

    /// Use a custom vtable for this environment instead of the default one.
    /// This allows you to override the default function definitions or add new ones.
    ///
    /// Tip: You can create a custom vtable by cloning the default one and modifying it, e.g.:
    /// ```rust
    /// use sosaku::{Value, Env, VTable, DEFAULT_VTABLE, FnArgs, FnResult, FnCallback, FnCallError, EvalError};
    ///
    /// fn my_func(args: FnArgs<'_>) -> FnResult<'_> {
    ///     // Your function implementation goes here
    ///     if !args.is_empty() {
    ///         return Err(EvalError::ArgumentCount {
    ///             expected: 0,
    ///             got: args.len(),
    ///         });
    ///     }
    ///
    ///     Ok(Value::Int(42))
    /// }
    ///
    /// # #[cfg(feature = "serde_json")] {
    /// let mut custom_vtable = DEFAULT_VTABLE.clone();
    /// custom_vtable.insert("my_func", FnCallback::new_sync(my_func));
    /// let env = Env::<serde_json::Value>::new()
    ///    .bind("x", serde_json::json!(42))
    ///    .use_vtable_ref(&custom_vtable)
    ///    .build();
    /// # }
    /// ```
    ///
    /// # Parameters
    ///
    /// - `vtable`: The custom vtable to use for this environment.
    ///
    /// # Returns
    ///
    /// A mutable reference to this [`EnvBuilder`] for method chaining.
    pub fn use_vtable_ref(&mut self, vtable: &'vtable VTable) -> &mut Self {
        self.vtable = Some(Cow::Borrowed(vtable));
        self
    }

    /// Finish the construction of the [`Env`] and return the final instance.
    ///
    /// This will clone the variable bindings and vtable from this builder into the new `Env`.
    /// However, since `EnvBuilder` is typically dropped after this, Rust is likely to optimize
    /// away the cloning of the bindings and vtable in release mode, so this should not have a
    /// significant performance impact in practice.
    ///
    /// # Returns
    ///
    /// An [`Env`] instance containing the variable bindings and vtable configured in this builder.
    #[must_use]
    pub fn build(&mut self) -> Env<'var, 'vtable, V> {
        // Rust is likely to optimize away the .clone() here since EnvBuilder is typically dropped after this
        // See: https://docs.rs/derive_builder/0.20.2/derive_builder/#-performance-considerations
        let vtable = self
            .vtable
            .clone()
            .unwrap_or_else(|| Cow::Borrowed(&*DEFAULT_VTABLE));

        Env {
            bindings: self.bindings.clone(),
            vtable,
        }
    }
}

impl<K, V, I> From<I> for Env<'_, '_, V>
where
    K: Into<Box<str>>,
    V: JsonValue + Clone + Debug,
    I: IntoIterator<Item = (K, V)>,
{
    fn from(value: I) -> Self {
        Env {
            bindings: value
                .into_iter()
                .map(|(k, v)| (k.into(), Cow::Owned(v)))
                .collect(),
            vtable: Cow::Borrowed(&*DEFAULT_VTABLE),
        }
    }
}
