use std::fmt;
use std::rc::Rc;

use crate::chunk::Function;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(Rc<str>),
    Array(Rc<Vec<Value>>),
    Unit,
    Function(Rc<Function>),
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
                    // Quotes keep `["a, b"]` apart from `["a", "b"]`.
                    match item {
                        Value::Str(s) => write!(f, "{s:?}")?,
                        _ => write!(f, "{item}")?,
                    }
                }
                write!(f, "]")
            }
            Value::Unit => write!(f, "()"),
            Value::Function(function) => write!(f, "<fn {}>", function.name),
        }
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
