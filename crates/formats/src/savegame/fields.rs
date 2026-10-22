//! An object's or block's values: the field list and its path lookups.

use super::*;

/// A decoded value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A bool.
    Bool(bool),
    /// An `i8`.
    I8(i8),
    /// An `i16`.
    I16(i16),
    /// A `u16`.
    U16(u16),
    /// An `i32`.
    I32(i32),
    /// A `u32`.
    U32(u32),
    /// An `i64`.
    I64(i64),
    /// An `f32`.
    F32(f32),
    /// A list of `f32`.
    F32s(Vec<f32>),
    /// A list of `i64`.
    I64s(Vec<i64>),
    /// A list of `u32`.
    U32s(Vec<u32>),
    /// A string.
    Str(String),
    /// An external reference id (`u32::MAX` for none).
    Ext(u32),
    /// An internal pointer to an object (none for null).
    Ptr(Option<ObjId>),
    /// A list of records.
    List(Vec<Fields>),
    /// A nested block: its version and fields.
    Block(u32, Fields),
}

/// Named values in serialization order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fields(pub Vec<(&'static str, Value)>);

impl Fields {
    /// A field of this block by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    /// A field inside nested blocks: `path` is `"block.field"`.
    pub fn at(&self, path: &str) -> Option<&Value> {
        let mut fields = self;
        let mut parts = path.split('.').peekable();
        while let Some(p) = parts.next() {
            let v = fields.get(p)?;
            if parts.peek().is_none() {
                return Some(v);
            }
            match v {
                Value::Block(_, f) => fields = f,
                _ => return None,
            }
        }
        None
    }

    /// [`Fields::at`] for writing.
    pub fn at_mut(&mut self, path: &str) -> Option<&mut Value> {
        let (head, rest) = match path.split_once('.') {
            Some((h, r)) => (h, Some(r)),
            None => (path, None),
        };
        let v = self
            .0
            .iter_mut()
            .find(|(n, _)| *n == head)
            .map(|(_, v)| v)?;
        match rest {
            None => Some(v),
            Some(rest) => match v {
                Value::Block(_, f) => f.at_mut(rest),
                _ => None,
            },
        }
    }

    /// Replace the value at `path`, keeping its kind (an `I32` stays an `I32`, a `Bool` that
    /// holds a raw byte takes the new truth value as 0 or 1 only if it changed). Returns whether
    /// the field exists.
    pub fn set(&mut self, path: &str, value: Value) -> bool {
        let Some(slot) = self.at_mut(path) else {
            return false;
        };
        // A raw non-0/1 bool byte stays as written while its truth value is unchanged.
        if let (Value::I8(b), Value::Bool(v)) = (&*slot, &value) {
            if (*b != 0) == *v {
                return true;
            }
        }
        *slot = value;
        true
    }

    /// Set an `i32` field inside nested blocks (`"block.field"`). Returns whether it exists.
    pub fn set_i32(&mut self, path: &str, v: i32) -> bool {
        match self.at_mut(path) {
            Some(Value::I32(x)) => *x = v,
            Some(Value::U32(x)) | Some(Value::Ext(x)) => *x = v as u32,
            Some(Value::I16(x)) => *x = v as i16,
            Some(Value::I8(x)) => *x = v as i8,
            Some(Value::U16(x)) => *x = v as u16,
            _ => return false,
        }
        true
    }

    /// Set an `f32` field inside nested blocks. Returns whether it exists.
    pub fn set_f32(&mut self, path: &str, v: f32) -> bool {
        match self.at_mut(path) {
            Some(Value::F32(x)) => *x = v,
            _ => return false,
        }
        true
    }

    /// Set a time field (microseconds) inside nested blocks. Returns whether it exists.
    pub fn set_time(&mut self, path: &str, v: i64) -> bool {
        match self.at_mut(path) {
            Some(Value::I64(x)) => *x = v,
            _ => return false,
        }
        true
    }

    /// Set a `bool` field inside nested blocks. Returns whether it exists.
    pub fn set_bool(&mut self, path: &str, v: bool) -> bool {
        self.set(path, Value::Bool(v))
    }

    /// An `i32` field inside nested blocks.
    pub fn i32(&self, path: &str) -> Option<i32> {
        match self.at(path)? {
            Value::I32(v) => Some(*v),
            Value::I16(v) => Some(*v as i32),
            Value::I8(v) => Some(*v as i32),
            Value::U32(v) | Value::Ext(v) => Some(*v as i32),
            _ => None,
        }
    }

    /// An `f32` field inside nested blocks.
    pub fn f32(&self, path: &str) -> Option<f32> {
        match self.at(path)? {
            Value::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// An object pointer field inside nested blocks.
    pub fn ptr(&self, path: &str) -> Option<ObjId> {
        match self.at(path)? {
            Value::Ptr(p) => *p,
            _ => None,
        }
    }
}
