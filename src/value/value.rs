use ordered_float::OrderedFloat;
use std::collections::BTreeMap;
use std::hash::Hash;

use serde::de::Deserialize;

use super::de::DeserializerError;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum Type {
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Char,
    String,
    List,
    Map,
    Bytes,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(OrderedFloat<f32>),
    F64(OrderedFloat<f64>),
    Char(char),
    String(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Bytes(Vec<u8>),
    Option(Option<Box<Value>>),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::Bool(_) => Type::Bool,
            Value::U8(_) => Type::U8,
            Value::U16(_) => Type::U16,
            Value::U32(_) => Type::U32,
            Value::U64(_) => Type::U64,
            Value::I8(_) => Type::I8,
            Value::I16(_) => Type::I16,
            Value::I32(_) => Type::I32,
            Value::I64(_) => Type::I64,
            Value::F32(_) => Type::F32,
            Value::F64(_) => Type::F64,
            Value::Char(_) => Type::Char,
            Value::String(_) => Type::String,
            Value::Option(_) => Type::None,
            Value::List(_) => Type::List,
            Value::Map(_) => Type::Map,
            Value::Bytes(_) => Type::Bytes,
        }
    }

    pub fn deserialize_into<'de, T: Deserialize<'de>>(self) -> Result<T, DeserializerError> {
        T::deserialize(self)
    }

    pub fn is_none(&self) -> bool {
        match self {
            Value::Option(None) => true,
            _ => false,
        }
    }

    pub fn into_string(self) -> Result<String, ()> {
        match self {
            Value::String(s) => Ok(s),
            _ => panic!("not a string"),
        }
    }
}

impl Value {
    pub(crate) fn unexpected(&self) -> serde::de::Unexpected {
        match *self {
            Value::Bool(b) => serde::de::Unexpected::Bool(b),
            Value::U8(n) => serde::de::Unexpected::Unsigned(n as u64),
            Value::U16(n) => serde::de::Unexpected::Unsigned(n as u64),
            Value::U32(n) => serde::de::Unexpected::Unsigned(n as u64),
            Value::U64(n) => serde::de::Unexpected::Unsigned(n),
            Value::I8(n) => serde::de::Unexpected::Signed(n as i64),
            Value::I16(n) => serde::de::Unexpected::Signed(n as i64),
            Value::I32(n) => serde::de::Unexpected::Signed(n as i64),
            Value::I64(n) => serde::de::Unexpected::Signed(n),
            Value::F32(n) => serde::de::Unexpected::Float(*n as f64),
            Value::F64(n) => serde::de::Unexpected::Float(*n),
            Value::Char(c) => serde::de::Unexpected::Char(c),
            Value::String(ref s) => serde::de::Unexpected::Str(s),
            Value::Option(_) => serde::de::Unexpected::Option,
            Value::List(_) => serde::de::Unexpected::Seq,
            Value::Map(_) => serde::de::Unexpected::Map,
            Value::Bytes(ref b) => serde::de::Unexpected::Bytes(b),
        }
    }
}

macro_rules! from_impl {
    ($from: ty, $map: ident) => {
        impl From<$from> for Value {
            fn from(from: $from) -> Value {
                Value::$map(from)
            }
        }
    };
}

from_impl!(bool, Bool);
from_impl!(u8, U8);
from_impl!(i8, I8);
from_impl!(u16, U16);
from_impl!(i16, I16);
from_impl!(i32, I32);
from_impl!(u32, U32);
from_impl!(i64, I64);
from_impl!(u64, U64);
from_impl!(String, String);
from_impl!(Vec<u8>, Bytes);

impl<'a> From<&'a str> for Value {
    fn from(s: &'a str) -> Value {
        Value::String(s.to_string())
    }
}

impl<'a> From<&'a [u8]> for Value {
    fn from(s: &'a [u8]) -> Value {
        Value::Bytes(s.to_owned())
    }
}

impl From<f32> for Value {
    fn from(s: f32) -> Value {
        Value::F32(s.into())
    }
}

impl From<f64> for Value {
    fn from(s: f64) -> Value {
        Value::F64(s.into())
    }
}
