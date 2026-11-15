use crate::ast::{Block, Expr, ExprKind, LogicalOp, Program, Stmt};
use crate::chunk::{Chunk, Op};
use crate::lexer::Pos;

pub fn compile(program: &Program) -> Chunk {
    let mut compiler = Compiler {
        chunk: Chunk::default(),
    };
    compiler.stmts(&program.stmts);
    if let Some(expr) = &program.expr {
        compiler.expr(expr);
    }
    compiler.chunk
}

struct Compiler {
    chunk: Chunk,
}

impl Compiler {
    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Expr(expr) => {
                    self.expr(expr);
                    self.chunk.emit(Op::Pop, expr.pos);
                }
                Stmt::Let { .. } | Stmt::Assign { .. } | Stmt::Fn(_) => {
                    todo!("variables and functions in the VM")
                }
            }
        }
    }

    fn block(&mut self, block: &Block, pos: Pos) {
        self.stmts(&block.stmts);
        match &block.expr {
            Some(value) => self.expr(value),
            None => {
                self.chunk.emit(Op::Unit, pos);
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        let pos = expr.pos;
        match &expr.kind {
            ExprKind::Int(n) => {
                self.chunk.emit(Op::Int(*n), pos);
            }
            ExprKind::Bool(b) => {
                self.chunk.emit(Op::Bool(*b), pos);
            }
            ExprKind::Unary { op, operand } => {
                self.expr(operand);
                self.chunk.emit(Op::Unary(op.clone()), pos);
            }
            ExprKind::Binary { op, left, right } => {
                self.expr(left);
                self.expr(right);
                self.chunk.emit(Op::Binary(op.clone()), pos);
            }
            ExprKind::Logical { op, left, right } => self.logical(op, left, right, pos),
            ExprKind::Block(block) => self.block(block, pos),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond);
                let to_else = self.chunk.emit(Op::JumpIfFalse(0), cond.pos);
                self.block(then_branch, pos);
                let to_end = self.chunk.emit(Op::Jump(0), pos);
                self.chunk.patch_jump(to_else);
                match else_branch {
                    Some(else_branch) => self.expr(else_branch),
                    None => {
                        self.chunk.emit(Op::Unit, pos);
                    }
                }
                self.chunk.patch_jump(to_end);
            }
            ExprKind::Var(_)
            | ExprKind::Call { .. }
            | ExprKind::While { .. }
            | ExprKind::Return(_)
            | ExprKind::Break
            | ExprKind::Continue => todo!("variables, loops and functions in the VM"),
        }
    }

    // Both operands must be bools: every operand goes through JumpIfFalse,
    // which reports a type error for anything else.
    fn logical(&mut self, op: &LogicalOp, left: &Expr, right: &Expr, pos: Pos) {
        let mut to_false = Vec::new();
        let mut to_true = Vec::new();

        self.expr(left);
        let left_false = self.chunk.emit(Op::JumpIfFalse(0), pos);
        match op {
            LogicalOp::And => to_false.push(left_false),
            LogicalOp::Or => {
                to_true.push(self.chunk.emit(Op::Jump(0), pos));
                self.chunk.patch_jump(left_false);
            }
        }

        self.expr(right);
        to_false.push(self.chunk.emit(Op::JumpIfFalse(0), pos));
        to_true.push(self.chunk.emit(Op::Jump(0), pos));

        for jump in to_false {
            self.chunk.patch_jump(jump);
        }
        self.chunk.emit(Op::Bool(false), pos);
        let to_end = self.chunk.emit(Op::Jump(0), pos);
        for jump in to_true {
            self.chunk.patch_jump(jump);
        }
        self.chunk.emit(Op::Bool(true), pos);
        self.chunk.patch_jump(to_end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinaryOp::{Add, Mul};
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn code(input: &str) -> Vec<Op> {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        compile(&program).code
    }

    #[test]
    fn arithmetic_is_postfix() {
        assert_eq!(
            code("1 + 2 * 3"),
            [
                Op::Int(1),
                Op::Int(2),
                Op::Int(3),
                Op::Binary(Mul),
                Op::Binary(Add)
            ]
        );
    }

    #[test]
    fn statement_pops_its_value() {
        assert_eq!(code("1; true"), [Op::Int(1), Op::Pop, Op::Bool(true)]);
    }

    #[test]
    fn if_else_jumps() {
        assert_eq!(
            code("if true { 1 } else { 2 }"),
            [
                Op::Bool(true),
                Op::JumpIfFalse(4),
                Op::Int(1),
                Op::Jump(5),
                Op::Int(2)
            ]
        );
    }

    #[test]
    fn if_without_else_is_unit() {
        assert_eq!(
            code("if true { 1 }"),
            [
                Op::Bool(true),
                Op::JumpIfFalse(4),
                Op::Int(1),
                Op::Jump(5),
                Op::Unit
            ]
        );
    }

    #[test]
    fn and_short_circuits() {
        assert_eq!(
            code("true && false"),
            [
                Op::Bool(true),
                Op::JumpIfFalse(5),
                Op::Bool(false),
                Op::JumpIfFalse(5),
                Op::Jump(7),
                Op::Bool(false),
                Op::Jump(8),
                Op::Bool(true)
            ]
        );
    }

    #[test]
    fn or_short_circuits() {
        assert_eq!(
            code("true || false"),
            [
                Op::Bool(true),
                Op::JumpIfFalse(3),
                Op::Jump(8),
                Op::Bool(false),
                Op::JumpIfFalse(6),
                Op::Jump(8),
                Op::Bool(false),
                Op::Jump(9),
                Op::Bool(true)
            ]
        );
    }

    #[test]
    fn positions_follow_code() {
        let program = Parser::new(tokenize("1 +\n  2").unwrap()).parse().unwrap();
        let chunk = compile(&program);
        let positions: Vec<String> = chunk.positions.iter().map(|p| p.to_string()).collect();
        assert_eq!(positions, ["1:1", "2:3", "1:3"]);
    }
}
