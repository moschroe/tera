use std::collections::BTreeMap;
use std::io::Write;

use serde::ser::Serialize;
use serde_json::value::{to_value, Map, Value};

use crate::errors::{Error, Result as TeraResult};
use crate::FunctionRelaxed;
use std::sync::Arc;

/// The struct that holds the context of a template rendering.
///
/// Light wrapper around a `BTreeMap` for easier insertions of Serializable
/// values
#[derive(Clone)]
pub struct Context {
    data: BTreeMap<String, Value>,
    /// Ignored by PartialEq!
    functions: BTreeMap<String, Arc<dyn FunctionRelaxed>>,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("data", &self.data)
            .field("functions", &self.functions.keys())
            .finish()
    }
}

impl PartialEq for Context {
    fn eq(&self, other: &Self) -> bool {
        self.data.eq(&other.data)
    }
}

impl Context {
    /// Initializes an empty context
    pub fn new() -> Self {
        Context { data: BTreeMap::new(), functions: Default::default() }
    }

    /// Converts the `val` parameter to `Value` and insert it into the context.
    ///
    /// Panics if the serialization fails.
    ///
    /// ```rust
    /// # use tera::Context;
    /// let mut context = tera::Context::new();
    /// context.insert("number_users", &42);
    /// ```
    pub fn insert<T: Serialize + ?Sized, S: Into<String>>(&mut self, key: S, val: &T) {
        self.data.insert(key.into(), to_value(val).unwrap());
    }

    /// Converts the `val` parameter to `Value` and insert it into the context.
    ///
    /// Returns an error if the serialization fails.
    ///
    /// ```rust
    /// # use tera::Context;
    /// # struct CannotBeSerialized;
    /// # impl serde::Serialize for CannotBeSerialized {
    /// #     fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    /// #         Err(serde::ser::Error::custom("Error"))
    /// #     }
    /// # }
    /// # let user = CannotBeSerialized;
    /// let mut context = Context::new();
    /// // user is an instance of a struct implementing `Serialize`
    /// if let Err(_) = context.try_insert("number_users", &user) {
    ///     // Serialization failed
    /// }
    /// ```
    pub fn try_insert<T: Serialize + ?Sized, S: Into<String>>(
        &mut self,
        key: S,
        val: &T,
    ) -> TeraResult<()> {
        self.data.insert(key.into(), to_value(val)?);

        Ok(())
    }

    /// Registers Context-local function
    pub fn register_function<T: FunctionRelaxed + 'static, S: Into<String>>(
        &mut self,
        key: S,
        val: T,
    ) {
        self.functions.insert(key.into(), Arc::new(val));
    }

    /// Appends the data of the `source` parameter to `self`, overwriting existing keys.
    /// The source context will be dropped.
    ///
    /// ```rust
    /// # use tera::Context;
    /// let mut target = Context::new();
    /// target.insert("a", &1);
    /// target.insert("b", &2);
    /// let mut source = Context::new();
    /// source.insert("b", &3);
    /// source.insert("d", &4);
    /// target.extend(source);
    /// ```
    pub fn extend(&mut self, mut source: Context) {
        self.data.append(&mut source.data);
    }

    /// Converts the context to a `serde_json::Value` consuming the context.
    pub fn into_json(self) -> Value {
        let mut m = Map::new();
        for (key, value) in self.data {
            m.insert(key, value);
        }
        Value::Object(m)
    }

    /// Takes a serde-json `Value` and convert it into a `Context` with no overhead/cloning.
    pub fn from_value(obj: Value) -> TeraResult<Self> {
        match obj {
            Value::Object(m) => {
                let mut data = BTreeMap::new();
                for (key, value) in m {
                    data.insert(key, value);
                }
                Ok(Context { data, functions: Default::default() })
            }
            _ => Err(Error::msg(
                "Creating a Context from a Value/Serialize requires it being a JSON object",
            )),
        }
    }

    /// Takes something that impl Serialize and create a context with it.
    /// Meant to be used if you have a hashmap or a struct and don't want to insert values
    /// one by one in the context.
    pub fn from_serialize(value: impl Serialize) -> TeraResult<Self> {
        let obj = to_value(value).map_err(Error::json)?;
        Context::from_value(obj)
    }

    /// Returns the value at a given key index.
    pub fn get(&self, index: &str) -> Option<&Value> {
        self.data.get(index)
    }

    /// Remove a key from the context, returning the value at the key if the key was previously inserted into the context.
    pub fn remove(&mut self, index: &str) -> Option<Value> {
        self.data.remove(index)
    }

    /// Checks if a value exists at a specific index.
    pub fn contains_key(&self, index: &str) -> bool {
        self.data.contains_key(index)
    }

    /// Looks up Context-local registered function
    #[inline]
    pub fn get_function(&self, fn_name: &str) -> Option<&Arc<dyn FunctionRelaxed>> {
        self.functions.get(fn_name)
    }
}

impl Default for Context {
    fn default() -> Context {
        Context::new()
    }
}

pub trait ValueRender {
    fn render(&self, write: &mut impl Write) -> std::io::Result<()>;
}

// Convert serde Value to String.
impl ValueRender for Value {
    fn render(&self, write: &mut impl Write) -> std::io::Result<()> {
        match *self {
            Value::String(ref s) => write!(write, "{}", s),
            Value::Number(ref i) => write!(write, "{}", i),
            Value::Bool(i) => write!(write, "{}", i),
            Value::Null => Ok(()),
            Value::Array(ref a) => {
                let mut first = true;
                write!(write, "[")?;
                for i in a.iter() {
                    if !first {
                        write!(write, ", ")?;
                    }
                    first = false;
                    i.render(write)?;
                }
                write!(write, "]")?;
                Ok(())
            }
            Value::Object(_) => write!(write, "[object]"),
        }
    }
}

pub trait ValueNumber {
    fn to_number(&self) -> Result<f64, ()>;
}
// Needed for all the maths
// Convert everything to f64, seems like a terrible idea
impl ValueNumber for Value {
    fn to_number(&self) -> Result<f64, ()> {
        match *self {
            Value::Number(ref i) => Ok(i.as_f64().unwrap()),
            _ => Err(()),
        }
    }
}

// From handlebars-rust
pub trait ValueTruthy {
    fn is_truthy(&self) -> bool;
}

impl ValueTruthy for Value {
    fn is_truthy(&self) -> bool {
        match *self {
            Value::Number(ref i) => {
                if i.is_i64() {
                    return i.as_i64().unwrap() != 0;
                }
                if i.is_u64() {
                    return i.as_u64().unwrap() != 0;
                }
                let f = i.as_f64().unwrap();
                f != 0.0 && !f.is_nan()
            }
            Value::Bool(ref i) => *i,
            Value::Null => false,
            Value::String(ref i) => !i.is_empty(),
            Value::Array(ref i) => !i.is_empty(),
            Value::Object(ref i) => !i.is_empty(),
        }
    }
}

/// Converts a dotted path to a json pointer one
#[inline]
pub fn get_json_pointer(key: &str) -> String {
    lazy_static::lazy_static! {
        // Split the key into dot-separated segments, respecting quoted strings as single units
        // to fix https://github.com/Keats/tera/issues/590
        static ref JSON_POINTER_REGEX: regex::Regex = regex::Regex::new("\"[^\"]*\"|[^.]+").unwrap();
    }

    let mut segments = vec![""];
    segments.extend(JSON_POINTER_REGEX.find_iter(key).map(|mat| mat.as_str().trim_matches('"')));
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn can_extend_context() {
        let mut target = Context::new();
        target.insert("a", &1);
        target.insert("b", &2);
        let mut source = Context::new();
        source.insert("b", &3);
        source.insert("c", &4);
        target.extend(source);
        assert_eq!(*target.data.get("a").unwrap(), to_value(1).unwrap());
        assert_eq!(*target.data.get("b").unwrap(), to_value(3).unwrap());
        assert_eq!(*target.data.get("c").unwrap(), to_value(4).unwrap());
    }

    #[test]
    fn can_create_context_from_value() {
        let obj = json!({
            "name": "bob",
            "age": 25
        });
        let context_from_value = Context::from_value(obj).unwrap();
        let mut context = Context::new();
        context.insert("name", "bob");
        context.insert("age", &25);
        assert_eq!(context_from_value, context);
    }

    #[test]
    fn can_create_context_from_impl_serialize() {
        let mut map = HashMap::new();
        map.insert("name", "bob");
        map.insert("last_name", "something");
        let context_from_serialize = Context::from_serialize(&map).unwrap();
        let mut context = Context::new();
        context.insert("name", "bob");
        context.insert("last_name", "something");
        assert_eq!(context_from_serialize, context);
    }

    #[test]
    fn can_remove_a_key() {
        let mut context = Context::new();
        context.insert("name", "foo");
        context.insert("bio", "Hi, I'm foo.");

        let mut expected = Context::new();
        expected.insert("name", "foo");
        assert_eq!(context.remove("bio"), Some(to_value("Hi, I'm foo.").unwrap()));
        assert_eq!(context.get("bio"), None);
        assert_eq!(context, expected);
    }

    #[test]
    fn remove_return_none_with_unknown_index() {
        let mut context = Context::new();
        assert_eq!(context.remove("unknown"), None);
    }
}
