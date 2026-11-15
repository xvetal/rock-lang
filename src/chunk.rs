use std::rc::Rc;

use crate::ast::{BinaryOp, UnaryOp};
use crate::lexer::Pos;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Int(i64),
    Bool(bool),
    Unit,
    Pop,
    DefineLocal,
    GetLocal(usize),
    SetLocal(usize),
    DropLocals(usize),
    Unary(UnaryOp),
    Binary(BinaryOp),
    Jump(usize),
    JumpIfFalse(usize),
    Function(usize),
    Call(usize),
    Return,
}

#[derive(Debug, Default, PartialEq)]
pub struct Chunk {
    pub code: Vec<Op>,
    pub positions: Vec<Pos>,
}

#[derive(Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub arity: usize,
    pub chunk: Chunk,
}

#[derive(Debug)]
pub struct Bytecode {
    pub main: Rc<Function>,
    pub functions: Vec<Rc<Function>>,
}

impl Chunk {
    pub fn emit(&mut self, op: Op, pos: Pos) -> usize {
        self.code.push(op);
        self.positions.push(pos);
        self.code.len() - 1
    }

    pub fn patch_jump(&mut self, at: usize) {
        let here = self.code.len();
        match &mut self.code[at] {
            Op::Jump(target) | Op::JumpIfFalse(target) => *target = here,
            op => panic!("cannot patch {op:?}: not a jump"),
        }
    }
}
