use std::cmp::Ordering;
use std::fmt;
use std::io;
use std::ops::Range;
use std::rc::Rc;

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
    IndexOutOfBounds { index: i64, len: usize },
    SliceOutOfBounds { index: i64, len: usize },
    SliceOrder { start: usize, end: usize },
    NotCharBoundary(usize),
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
            RuntimeErrorKind::IndexOutOfBounds { index, len } => {
                write!(f, "index {index} out of bounds for length {len}")
            }
            RuntimeErrorKind::SliceOutOfBounds { index, len } => {
                write!(f, "slice index {index} out of bounds for length {len}")
            }
            RuntimeErrorKind::SliceOrder { start, end } => {
                write!(f, "slice starts at {start} but ends at {end}")
            }
            RuntimeErrorKind::NotCharBoundary(index) => {
                write!(f, "byte index {index} is not a char boundary")
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
        BinaryOp::Lt => Ok(Value::Bool(order(&l, &r).is_lt())),
        BinaryOp::Le => Ok(Value::Bool(order(&l, &r).is_le())),
        BinaryOp::Gt => Ok(Value::Bool(order(&l, &r).is_gt())),
        BinaryOp::Ge => Ok(Value::Bool(order(&l, &r).is_ge())),
        BinaryOp::Add => match (l, r) {
            (Value::Str(l), Value::Str(r)) => Ok(Value::Str(format!("{l}{r}").into())),
            (l, r) => checked(l.as_int().checked_add(r.as_int())),
        },
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

// A string is sliced by bytes, but only between characters.
pub fn slice(
    value: Value,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<Value, RuntimeErrorKind> {
    match value {
        Value::Str(s) => {
            let range = slice_range(start, end, s.len())?;
            let inside = [range.start, range.end]
                .into_iter()
                .find(|&i| !s.is_char_boundary(i));
            if let Some(index) = inside {
                return Err(RuntimeErrorKind::NotCharBoundary(index));
            }
            Ok(Value::Str(s[range].into()))
        }
        Value::Array(items) => {
            let range = slice_range(start, end, items.len())?;
            Ok(Value::Array(Rc::new(items[range].to_vec())))
        }
        other => panic!("the checker rejects slicing {other}"),
    }
}

fn slice_range(
    start: Option<i64>,
    end: Option<i64>,
    len: usize,
) -> Result<Range<usize>, RuntimeErrorKind> {
    let bound = |index: Option<i64>, missing: usize| match index {
        None => Ok(missing),
        Some(index) => usize::try_from(index)
            .ok()
            .filter(|&i| i <= len)
            .ok_or(RuntimeErrorKind::SliceOutOfBounds { index, len }),
    };
    let (start, end) = (bound(start, 0)?, bound(end, len)?);
    if start > end {
        return Err(RuntimeErrorKind::SliceOrder { start, end });
    }
    Ok(start..end)
}

fn checked(result: Option<i64>) -> Result<Value, RuntimeErrorKind> {
    result
        .map(Value::Int)
        .ok_or(RuntimeErrorKind::IntegerOverflow)
}

fn order(l: &Value, r: &Value) -> Ordering {
    match (l, r) {
        (Value::Str(l), Value::Str(r)) => l.cmp(r),
        _ => l.as_int().cmp(&r.as_int()),
    }
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
                RuntimeErrorKind::IndexOutOfBounds { index: -1, len: 3 },
                "index -1 out of bounds for length 3",
            ),
            (
                RuntimeErrorKind::SliceOutOfBounds { index: 4, len: 3 },
                "slice index 4 out of bounds for length 3",
            ),
            (
                RuntimeErrorKind::SliceOrder { start: 2, end: 1 },
                "slice starts at 2 but ends at 1",
            ),
            (
                RuntimeErrorKind::NotCharBoundary(1),
                "byte index 1 is not a char boundary",
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
