use std::{
    borrow::Cow,
    fmt::{Debug, Display, Write},
};

use nom::Finish;

#[cfg(feature = "serde_json")]
use serde::Deserialize;

use crate::{
    VarAccessError,
    types::{
        env::Env,
        json::{JsonMap, JsonValue},
    },
};

use crate::parser::parse_variable_name;

/// A variable name, with an optional index for array access.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarName {
    name: Box<str>,
    index: Option<usize>,
}

impl VarName {
    /// Create a new [`VarName`] with the given name and optional index.
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the variable.
    /// - `index`: An optional index for array access,
    ///   if this variable name is used to access an array element
    ///   (e.g. `foo[0]` would have name "foo" and index 0).
    ///
    /// # Returns
    ///
    /// - A new [`VarName`] instance containing the provided name and index.
    pub fn new(name: impl Into<Box<str>>, index: Option<usize>) -> Self {
        Self {
            name: name.into(),
            index,
        }
    }

    /// The name of the variable.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The optional index for array access, if this variable name is used to access an array element.
    pub const fn index(&self) -> Option<usize> {
        self.index
    }

    fn access<'a, V: JsonValue + Debug>(
        &self,
        value: &'a V,
        resolve_obj_name: impl Fn() -> String,
    ) -> Result<&'a V, VarAccessError> {
        // Map into the object with the name
        if let Some(o) = value.as_object() {
            let out = o
                .get(self.name())
                .ok_or_else(|| VarAccessError::ObjectKeyError {
                    object: resolve_obj_name(),
                    key: self.name().to_string(),
                })?;
            // If we have an index, index into the array
            if self.index().is_some() {
                self.index_into(out, resolve_obj_name)
            } else {
                Ok(out)
            }
        } else {
            // Trying to varaccess into a non-object value is always an error
            Err(VarAccessError::ObjectKeyError {
                object: resolve_obj_name(),
                key: self.name().to_string(),
            })
        }
    }

    fn access_mut<'a, V: JsonValue + Debug>(
        &self,
        value: &'a mut V,
        resolve_obj_name: impl Fn() -> String,
    ) -> Result<&'a mut V, VarAccessError> {
        // Map into the object with the name
        if let Some(o) = value.as_object_mut() {
            let out = o
                .get_mut(self.name())
                .ok_or_else(|| VarAccessError::ObjectKeyError {
                    object: resolve_obj_name(),
                    key: self.name().to_string(),
                })?;
            // If we have an index, index into the array
            if self.index().is_some() {
                self.index_into_mut(out, resolve_obj_name)
            } else {
                Ok(out)
            }
        } else {
            // Trying to varaccess into a non-object value is always an error
            Err(VarAccessError::ObjectKeyError {
                object: resolve_obj_name(),
                key: self.name().to_string(),
            })
        }
    }

    fn index_into<'a, V: JsonValue + Debug>(
        &self,
        value: &'a V,
        resolve_obj_name: impl Fn() -> String,
    ) -> Result<&'a V, VarAccessError> {
        if let Some(index) = self.index() {
            let arr = value.as_array().ok_or_else(|| VarAccessError::TypeError {
                message: format!(
                    "Expected array at '{}', found {:?}",
                    resolve_obj_name(),
                    value
                ),
            })?;

            arr.get(index)
                .ok_or_else(|| VarAccessError::IndexOutOfBounds {
                    message: format!(
                        "Index out of bounds at '{}' (index: {index}, length: {})",
                        resolve_obj_name(),
                        arr.len()
                    ),
                })
        } else {
            panic!("Called index_into on VarName without an index")
        }
    }

    fn index_into_mut<'a, V: JsonValue + Debug>(
        &self,
        value: &'a mut V,
        resolve_obj_name: impl Fn() -> String,
    ) -> Result<&'a mut V, VarAccessError> {
        if let Some(index) = self.index() {
            let arr = value
                .as_array_mut()
                .ok_or_else(|| VarAccessError::TypeError {
                    message: format!("Expected array at '{}'", resolve_obj_name()),
                })?;

            let len = arr.len();

            arr.get_mut(index)
                .ok_or_else(|| VarAccessError::IndexOutOfBounds {
                    message: format!(
                        "Index out of bounds at '{}' (index: {index}, length: {})",
                        resolve_obj_name(),
                        len
                    ),
                })
        } else {
            panic!("Called index_into on VarName without an index")
        }
    }
}

impl Display for VarName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(index) = self.index {
            write!(f, "[{index}]")?;
        }
        Ok(())
    }
}

/// A variable access, which is a series of variable names.
///
/// Example: `foo.bar[0].baz` would be represented as a `VarAccess` with three `VarName`s:
/// - `foo` with no index
/// - `bar` with index 0
/// - `baz` with no index
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarAccess {
    names: Vec<VarName>,
}

impl VarAccess {
    /// Create a new [`VarAccess`] from a vector of [`VarName`]s.
    ///
    /// # Panics
    ///
    /// This function will panic if the `names` vector is empty, as a variable access must have at least one name.
    pub fn new(names: impl Into<Vec<VarName>>) -> Self {
        let names = names.into();
        assert!(
            !names.is_empty(),
            "Variable access must have at least one name"
        );

        Self { names }
    }

    /// Get the sequence variable names in this access.
    pub fn names(&self) -> &[VarName] {
        &self.names
    }

    /// Resolve the variable access until the `i`th name, returning a string
    /// representation of the access path up to that point.
    fn resolve_name_until(names: &[VarName], root: Option<&VarName>, i: usize) -> String {
        if i == 0 {
            #[expect(clippy::or_fun_call)]
            root.unwrap_or(&VarName::new("<root>", None)).to_string()
        } else {
            let names = names[..i]
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();

            if let Some(r) = root {
                format!("{}.{}", r, names.join("."))
            } else {
                names.join(".")
            }
        }
    }

    fn access_names<'a, V: JsonValue + Debug>(
        mut names: &[VarName],
        value: &'a V,
        ignore_first: bool,
    ) -> Result<&'a V, VarAccessError> {
        // (curr_value, curr_name), root_name)
        let (mut current, root) = if ignore_first {
            let root = names
                .first()
                .expect("Variable access must have at least one name");
            names = names.get(1..).ok_or(VarAccessError::EmptyAccess)?;
            ((value, root), Some(root))
        } else {
            let val = names
                .first()
                .expect("Variable access must have at least one name");
            ((value, val), None)
        };

        // Reduce "current" by accessing each variable name in the access path
        for (i, var) in names.iter().enumerate() {
            current = (
                var.access(current.0, || Self::resolve_name_until(names, root, i))?,
                var,
            );
        }

        // Account for edge-case where if ignore_first is true and the first variable name has an index,
        // we need to access that index in the root value
        if names.is_empty() && ignore_first && current.1.index().is_some() {
            current.0 = current.1.index_into(current.0, || {
                Self::resolve_name_until(names, root, names.len())
            })?;
        }

        Ok(current.0)
    }

    fn access_names_mut<'a, V: JsonValue + Debug>(
        mut names: &[VarName],
        value: &'a mut V,
        ignore_first: bool,
    ) -> Result<&'a mut V, VarAccessError> {
        // (curr_value, curr_name), root_name)
        let (mut current, root) = if ignore_first {
            let root = names
                .first()
                .expect("Variable access must have at least one name");
            names = names.get(1..).ok_or(VarAccessError::EmptyAccess)?;
            ((value, root), Some(root))
        } else {
            let val = names
                .first()
                .expect("Variable access must have at least one name");
            ((value, val), None)
        };

        // Reduce "current" by accessing each variable name in the access path
        for (i, var) in names.iter().enumerate() {
            current = (
                var.access_mut(current.0, || Self::resolve_name_until(names, root, i))?,
                var,
            );
        }

        // Account for edge-case where if ignore_first is true and the first variable name has an index,
        // we need to access that index in the root value
        if names.is_empty() && ignore_first && current.1.index().is_some() {
            current.0 = current.1.index_into_mut(current.0, || {
                Self::resolve_name_until(names, root, names.len())
            })?;
        }

        Ok(current.0)
    }

    /// Access the value denoted by this accessor from the given JSON value.
    ///
    /// # Returns
    /// The value accessed from the provided JSON value according to
    /// the variable access specified by this [`VarAccess`].
    ///
    /// # Errors
    /// - If there was an error accessing the value, such as a type mismatch or index out of bounds
    pub fn access<'a, V: JsonValue + Debug>(&self, value: &'a V) -> Result<&'a V, VarAccessError> {
        Self::access_names(&self.names, value, false)
    }

    /// Access the value denoted by this accessor from the given JSON value and return a mutable reference to it.
    ///
    /// # Returns
    /// The value accessed from the provided JSON value according to
    /// the variable access specified by this [`VarAccess`].
    ///
    /// # Errors
    /// - If there was an error accessing the value, such as a type mismatch or index out of bounds
    pub fn access_mut<'a, V: JsonValue + Debug>(
        &self,
        value: &'a mut V,
    ) -> Result<&'a mut V, VarAccessError> {
        Self::access_names_mut(&self.names, value, false)
    }

    /// Replace the value denoted by this accessor in the given JSON value with the provided replacement value.
    ///
    /// # Errors
    ///
    /// If there was an error accessing the value to be replaced, such as a type mismatch or index out of bounds
    pub fn replace<V: JsonValue + Debug>(
        &self,
        value: &mut V,
        replacement: V,
    ) -> Result<V, VarAccessError> {
        let target = self.access_mut(value)?;
        Ok(std::mem::replace(target, replacement))
    }

    /// Access the value denoted by this accessor from the given JSON value.
    ///
    /// # Returns
    /// The value accessed from the provided JSON value according to
    /// the variable access specified by this [`VarAccess`].
    ///
    /// # Errors
    /// - If there was an error accessing the value, such as a type mismatch or index out of bounds
    pub fn access_from_bindings<'a, V: JsonValue + Debug + Clone>(
        &self,
        env: &'a Env<'a, '_, V>,
    ) -> Result<Cow<'a, V>, VarAccessError> {
        if self.names.is_empty() {
            return Ok(Cow::Owned(V::null()));
        }

        let first_name = self.names[0].name();
        let value =
            env.bindings()
                .get(first_name)
                .ok_or_else(|| VarAccessError::VariableNotFound {
                    variable: first_name.to_string(),
                })?;

        Self::access_names(&self.names, value.as_ref(), true).map(Cow::Borrowed)
    }

    /// Access the value denoted by this accessor from the given JSON value and return a mutable reference to it.
    ///
    /// # Returns
    /// The value accessed from the provided JSON value according to
    /// the variable access specified by this [`VarAccess`].
    ///
    /// # Errors
    /// - If there was an error accessing the value, such as a type mismatch or index out of bounds
    pub fn access_mut_from_bindings<'a, V: JsonValue + Debug + Clone>(
        &self,
        env: &'a mut Env<'a, '_, V>,
    ) -> Result<Cow<'a, V>, VarAccessError> {
        if self.names.is_empty() {
            return Ok(Cow::Owned(V::null()));
        }

        let first_name = self.names[0].name();
        let value = env.bindings_mut().get_mut(first_name).ok_or_else(|| {
            VarAccessError::VariableNotFound {
                variable: first_name.to_string(),
            }
        })?;

        Self::access_names_mut(&self.names, value.to_mut(), true)
            .map(|arg0: &mut V| Cow::Borrowed(&*arg0))
    }
}

impl Display for VarAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, name) in self.names.iter().enumerate() {
            if i > 0 {
                f.write_char('.')?;
            }
            write!(f, "{name}")?;
        }
        Ok(())
    }
}

impl<'a> TryFrom<&'a str> for VarAccess {
    type Error = nom::error::Error<&'a str>;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        match parse_variable_name(s).finish() {
            Ok(("", var_access)) => Ok(var_access),
            Ok((remaining, _)) => Err(nom::error::Error::new(
                remaining,
                nom::error::ErrorKind::Eof,
            )),
            Err(e) => Err(e),
        }
    }
}

#[cfg(feature = "serde_json")]
impl<'a> Deserialize<'a> for VarAccess {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from(s.as_str()).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[cfg(feature = "serde_json")]
mod tests {
    use std::sync::LazyLock;

    use serde_json::json;

    use super::*;

    static TEST_VALUE_1: LazyLock<serde_json::Value> = LazyLock::new(|| {
        serde_json::json!(
            {
                "foo": {
                    "bar": [
                        {"baz": 42},
                        {"baz": 43}
                    ]
                }
            }
        )
    });

    static TEST_VALUE_2: LazyLock<serde_json::Value> = LazyLock::new(|| {
        serde_json::json!(
            {
                "foo": {
                    "bar": [
                        {"baz": 42},
                        {"baz": 43}
                    ]
                },
                "arr": [1, 2, 3],
                "null_value": null,
                "string_value": "hello",
                "bool_value": true,
                "float_value": 3.145
            }
        )
    });

    #[test]
    fn test_var_access() {
        let var_access = VarAccess::try_from("foo.bar[0].baz").unwrap();
        let result = var_access.access(&*TEST_VALUE_1).unwrap();
        assert_eq!(*result, json!(42));
    }

    #[test]
    fn test_var_access_root_index() {
        let var_access = VarAccess::try_from("arr[1]").unwrap();
        let result = var_access.access(&*TEST_VALUE_2).unwrap();
        assert_eq!(*result, json!(2));
    }

    #[test]
    fn test_var_access_array() {
        let var_access = VarAccess::try_from("foo.bar").unwrap();
        let result = var_access.access(&*TEST_VALUE_1).unwrap();
        assert_eq!(*result, json!([{"baz": 42}, {"baz": 43}]));
    }

    #[test]
    fn test_var_access_object() {
        let var_access = VarAccess::try_from("foo").unwrap();
        let result = var_access.access(&*TEST_VALUE_1).unwrap();
        assert_eq!(
            *result,
            json!({
                "bar": [
                    {"baz": 42},
                    {"baz": 43}
                ]
            })
        );
    }

    #[test]
    fn test_var_access_null() {
        let var_access = VarAccess::try_from("null_value").unwrap();
        let result = var_access.access(&*TEST_VALUE_2).unwrap();
        assert_eq!(*result, json!(null));
    }

    #[test]
    fn test_var_replace() {
        let mut value = TEST_VALUE_1.clone();
        let var_access = VarAccess::try_from("foo.bar[0].baz").unwrap();
        let old_value = var_access
            .replace(&mut value, json!({"replacement": 100}))
            .unwrap();
        let var_access = VarAccess::try_from("foo.bar[0].baz.replacement").unwrap();
        let result = var_access.access(&value).unwrap();
        assert_eq!(old_value, json!(42));
        assert_eq!(*result, json!(100));
    }

    #[test]
    fn test_var_access_from_bindings() {
        let env = Env::<serde_json::Value>::new()
            .bind_ref("test", &*TEST_VALUE_1)
            .bind_ref("other", &*TEST_VALUE_2)
            .build();

        let var_access = VarAccess::try_from("test.foo.bar[1].baz").unwrap();
        let result = var_access.access_from_bindings(&env).unwrap();
        assert_eq!(*result, json!(43));

        let var_access = VarAccess::try_from("other.arr[1]").unwrap();
        let result = var_access.access_from_bindings(&env).unwrap();
        assert_eq!(*result, json!(2));
    }

    #[test]
    fn test_var_access_errors() {
        let var_access = VarAccess::try_from("foo.bar[2].baz").unwrap();
        let result = var_access.access(&*TEST_VALUE_1);
        assert!(matches!(
            result,
            Err(VarAccessError::IndexOutOfBounds { .. })
        ));

        let var_access = VarAccess::try_from("foo.baz").unwrap();
        let result = var_access.access(&*TEST_VALUE_1);
        assert!(matches!(result, Err(VarAccessError::ObjectKeyError { .. })));

        let var_access = VarAccess::try_from("foo.bar[0].baz.qux").unwrap();
        let result = var_access.access(&*TEST_VALUE_1);
        assert!(matches!(result, Err(VarAccessError::ObjectKeyError { .. })));

        let var_access = VarAccess::try_from("foo[0]").unwrap();
        let result = var_access.access(&*TEST_VALUE_1);
        assert!(matches!(result, Err(VarAccessError::TypeError { .. })));
    }
}
