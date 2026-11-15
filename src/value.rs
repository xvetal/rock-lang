use std::fmt;
use std::rc::Rc;

use crate::ast::FnDecl;
use crate::chunk::Function;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Unit,
    Function(Rc<FnDecl>),
    VmFunction(Rc<Function>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Unit => write!(f, "()"),
            Value::Function(decl) => write!(f, "<fn {}>", decl.name),
            Value::VmFunction(function) => write!(f, "<fn {}>", function.name),
        }
    }
}
