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
        UnaryOp::Neg => checked(value.as_int().checked_neg()),
        UnaryOp::Not => Ok(Value::Bool(!value.as_bool())),
    }
}

pub fn binary(op: &BinaryOp, l: Value, r: Value) -> Result<Value, RuntimeErrorKind> {
    match op {
        BinaryOp::Eq => Ok(Value::Bool(l == r)),
        BinaryOp::Ne => Ok(Value::Bool(l != r)),
        BinaryOp::Lt => Ok(Value::Bool(l.as_int() < r.as_int())),
        BinaryOp::Le => Ok(Value::Bool(l.as_int() <= r.as_int())),
        BinaryOp::Gt => Ok(Value::Bool(l.as_int() > r.as_int())),
        BinaryOp::Ge => Ok(Value::Bool(l.as_int() >= r.as_int())),
        BinaryOp::Add => checked(l.as_int().checked_add(r.as_int())),
        BinaryOp::Sub => checked(l.as_int().checked_sub(r.as_int())),
        BinaryOp::Mul => checked(l.as_int().checked_mul(r.as_int())),
        BinaryOp::Div => {
            let (l, r) = (l.as_int(), r.as_int());
            if r == 0 {
                return Err(RuntimeErrorKind::DivisionByZero);
            }
            checked(l.checked_div(r))
        }
    }
}

fn checked(result: Option<i64>) -> Result<Value, RuntimeErrorKind> {
    result
        .map(Value::Int)
        .ok_or(RuntimeErrorKind::IntegerOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages() {
        let messages = [
            (RuntimeErrorKind::DivisionByZero, "division by zero"),
            (RuntimeErrorKind::IntegerOverflow, "integer overflow"),
            (
                RuntimeErrorKind::StackOverflow,
                "stack overflow: more than 1000 nested calls",
            ),
            (
                RuntimeErrorKind::Output(io::ErrorKind::BrokenPipe),
                "cannot write output: broken pipe",
            ),
        ];
        for (kind, message) in messages {
            assert_eq!(kind.to_string(), message);
        }
    }
}
