use std::fmt;
use std::io;

use crate::ast::{BinaryOp, UnaryOp};
use crate::lexer::Pos;
use crate::value::Value;

pub const MAX_CALL_DEPTH: usize = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    pub pos: Pos,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeErrorKind {
    UndefinedVariable(String),
    UndefinedFunction(String),
    ArgumentCount {
        name: String,
        expected: usize,
        found: usize,
    },
    TypeMismatch {
        expected: &'static str,
        found: Value,
    },
    CannotCompare(Value, Value),
    DivisionByZero,
    IntegerOverflow,
    StackOverflow,
    Output(io::ErrorKind),
}

impl RuntimeErrorKind {
    pub fn at(self, pos: Pos) -> RuntimeError {
        RuntimeError { kind: self, pos }
    }
}

impl fmt::Display for RuntimeErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuntimeErrorKind::UndefinedVariable(name) => write!(f, "undefined variable: {name}"),
            RuntimeErrorKind::UndefinedFunction(name) => write!(f, "undefined function: {name}"),
            RuntimeErrorKind::ArgumentCount {
                name,
                expected,
                found,
            } => {
                let s = if *expected == 1 { "" } else { "s" };
                write!(f, "{name} expects {expected} argument{s}, got {found}")
            }
            RuntimeErrorKind::TypeMismatch { expected, found } => {
                write!(f, "expected {expected}, got {found}")
            }
            RuntimeErrorKind::CannotCompare(l, r) => write!(f, "cannot compare {l} with {r}"),
            RuntimeErrorKind::DivisionByZero => write!(f, "division by zero"),
            RuntimeErrorKind::IntegerOverflow => write!(f, "integer overflow"),
            RuntimeErrorKind::StackOverflow => {
                write!(f, "stack overflow: more than {MAX_CALL_DEPTH} nested calls")
            }
            RuntimeErrorKind::Output(kind) => write!(f, "cannot write output: {kind}"),
        }
    }
}

pub fn unary(op: &UnaryOp, value: Value) -> Result<Value, RuntimeErrorKind> {
    match op {
        UnaryOp::Neg => checked(int(value)?.checked_neg()),
        UnaryOp::Not => Ok(Value::Bool(!boolean(value)?)),
    }
}

pub fn binary(op: &BinaryOp, l: Value, r: Value) -> Result<Value, RuntimeErrorKind> {
    match op {
        BinaryOp::Eq => Ok(Value::Bool(equal(l, r)?)),
        BinaryOp::Ne => Ok(Value::Bool(!equal(l, r)?)),
        BinaryOp::Lt => Ok(Value::Bool(int(l)? < int(r)?)),
        BinaryOp::Le => Ok(Value::Bool(int(l)? <= int(r)?)),
        BinaryOp::Gt => Ok(Value::Bool(int(l)? > int(r)?)),
        BinaryOp::Ge => Ok(Value::Bool(int(l)? >= int(r)?)),
        BinaryOp::Add => checked(int(l)?.checked_add(int(r)?)),
        BinaryOp::Sub => checked(int(l)?.checked_sub(int(r)?)),
        BinaryOp::Mul => checked(int(l)?.checked_mul(int(r)?)),
        BinaryOp::Div => {
            let (l, r) = (int(l)?, int(r)?);
            if r == 0 {
                return Err(RuntimeErrorKind::DivisionByZero);
            }
            checked(l.checked_div(r))
        }
    }
}

fn equal(l: Value, r: Value) -> Result<bool, RuntimeErrorKind> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::Unit, Value::Unit) => Ok(true),
        (l, r) => Err(RuntimeErrorKind::CannotCompare(l, r)),
    }
}

fn int(value: Value) -> Result<i64, RuntimeErrorKind> {
    match value {
        Value::Int(n) => Ok(n),
        found => Err(RuntimeErrorKind::TypeMismatch {
            expected: "integer",
            found,
        }),
    }
}

pub fn boolean(value: Value) -> Result<bool, RuntimeErrorKind> {
    match value {
        Value::Bool(b) => Ok(b),
        found => Err(RuntimeErrorKind::TypeMismatch {
            expected: "bool",
            found,
        }),
    }
}

fn checked(result: Option<i64>) -> Result<Value, RuntimeErrorKind> {
    result
        .map(Value::Int)
        .ok_or(RuntimeErrorKind::IntegerOverflow)
}
