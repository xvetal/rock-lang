use crate::ast::{BinaryOp, UnaryOp};
use crate::lexer::Pos;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Int(i64),
    Bool(bool),
    Unit,
    Pop,
    DefineGlobal,
    GetGlobal(usize),
    SetGlobal(usize),
    DefineLocal,
    GetLocal(usize),
    SetLocal(usize),
    DropLocals(usize),
    Unary(UnaryOp),
    Binary(BinaryOp),
    Jump(usize),
    JumpIfFalse(usize),
}

#[derive(Debug, Default)]
pub struct Chunk {
    pub code: Vec<Op>,
    pub positions: Vec<Pos>,
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
