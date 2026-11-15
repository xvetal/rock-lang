use std::fmt;
use std::rc::Rc;

use crate::chunk::Function;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
    Function(Rc<Function>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
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
