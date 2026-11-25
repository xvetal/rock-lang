use std::fmt;
use std::rc::Rc;

use crate::chunk::Function;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(Rc<str>),
    Array(Rc<Vec<Value>>),
    Struct(Rc<Struct>),
    Unit,
    Function(Rc<Function>),
}

#[derive(Debug, PartialEq)]
pub struct Struct {
    pub def: Rc<StructDef>,
    // In the order of `def.fields`.
    pub fields: Vec<Value>,
}

// What a struct value needs to know about its declaration to be printed.
#[derive(Debug, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<String>,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write_inner(f, item)?;
                }
                write!(f, "]")
            }
            Value::Struct(value) => {
                write!(f, "{} {{", value.def.name)?;
                for (i, (name, field)) in value.def.fields.iter().zip(&value.fields).enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, " {name}: ")?;
                    write_inner(f, field)?;
                }
                if !value.fields.is_empty() {
                    write!(f, " ")?;
                }
                write!(f, "}}")
            }
            Value::Unit => write!(f, "()"),
            Value::Function(function) => write!(f, "<fn {}>", function.name),
        }
    }
}

// Quotes keep `["a, b"]` apart from `["a", "b"]`.
fn write_inner(f: &mut fmt::Formatter, value: &Value) -> fmt::Result {
    match value {
        Value::Str(s) => write!(f, "{s:?}"),
        _ => write!(f, "{value}"),
    }
}

// The checker guarantees the operand types, so a mismatch here is a bug.
impl Value {
    pub fn as_int(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            other => panic!("expected int, found {other}"),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            other => panic!("expected bool, found {other}"),
        }
    }
}
