use std::fmt;
use std::rc::Rc;

use crate::ast::{Block, Expr, ExprKind, FnDecl, LogicalOp, Program, Stmt};
use crate::chunk::{Bytecode, Chunk, Function, Op};
use crate::lexer::Pos;

#[derive(Debug, Clone, PartialEq)]
pub struct CompileError {
    pub kind: CompileErrorKind,
    pub pos: Pos,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompileErrorKind {
    UndefinedVariable(String),
    UndefinedFunction(String),
    DuplicateFunction(String),
}

impl CompileErrorKind {
    fn at(self, pos: Pos) -> CompileError {
        CompileError { kind: self, pos }
    }
}

impl fmt::Display for CompileErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompileErrorKind::UndefinedVariable(name) => write!(f, "undefined variable: {name}"),
            CompileErrorKind::UndefinedFunction(name) => write!(f, "undefined function: {name}"),
            CompileErrorKind::DuplicateFunction(name) => write!(f, "duplicate function: {name}"),
        }
    }
}

pub fn compile(program: &Program) -> Result<Bytecode, CompileError> {
    let decls: Vec<&FnDecl> = program
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Fn(decl) => Some(decl.as_ref()),
            _ => None,
        })
        .collect();
    for (i, decl) in decls.iter().enumerate() {
        if decls[..i].iter().any(|other| other.name == decl.name) {
            return Err(CompileErrorKind::DuplicateFunction(decl.name.clone()).at(decl.pos));
        }
    }

    let functions = decls
        .iter()
        .map(|decl| function(decl, &decls))
        .collect::<Result<_, _>>()?;

    let mut compiler = Compiler::new(&decls, Vec::new());
    compiler.stmts(&program.stmts)?;
    if let Some(expr) = &program.expr {
        compiler.expr(expr)?;
    }
    let main = Function {
        name: "main".to_string(),
        arity: 0,
        chunk: compiler.chunk,
    };
    Ok(Bytecode {
        main: Rc::new(main),
        functions,
    })
}

fn function(decl: &FnDecl, decls: &[&FnDecl]) -> Result<Rc<Function>, CompileError> {
    let mut compiler = Compiler::new(decls, decl.params.clone());
    compiler.block(&decl.body, decl.pos)?;
    compiler.chunk.emit(Op::Return, decl.pos);
    Ok(Rc::new(Function {
        name: decl.name.clone(),
        arity: decl.params.len(),
        chunk: compiler.chunk,
    }))
}

struct Compiler<'a> {
    functions: &'a [&'a FnDecl],
    chunk: Chunk,
    locals: Vec<String>,
    temps: usize,
    loops: Vec<Loop>,
}

struct Loop {
    start: usize,
    temps: usize,
    locals: usize,
    breaks: Vec<usize>,
}

impl<'a> Compiler<'a> {
    fn new(functions: &'a [&'a FnDecl], locals: Vec<String>) -> Self {
        Self {
            functions,
            chunk: Chunk::default(),
            locals,
            temps: 0,
            loops: Vec::new(),
        }
    }

    fn stmts(&mut self, stmts: &[Stmt]) -> Result<(), CompileError> {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, value } => {
                    self.expr(value)?;
                    self.chunk.emit(Op::DefineLocal, value.pos);
                    self.locals.push(name.clone());
                }
                Stmt::Assign { name, value, pos } => {
                    self.expr(value)?;
                    let slot = self.local(name).ok_or_else(|| {
                        CompileErrorKind::UndefinedVariable(name.clone()).at(*pos)
                    })?;
                    self.chunk.emit(Op::SetLocal(slot), *pos);
                }
                Stmt::Expr(expr) => {
                    self.expr(expr)?;
                    self.chunk.emit(Op::Pop, expr.pos);
                }
                Stmt::Fn(_) => {}
            }
        }
        Ok(())
    }

    fn block(&mut self, block: &Block, pos: Pos) -> Result<(), CompileError> {
        let outer = self.locals.len();
        self.stmts(&block.stmts)?;
        match &block.expr {
            Some(value) => self.expr(value)?,
            None => {
                self.chunk.emit(Op::Unit, pos);
            }
        }
        let count = self.locals.len() - outer;
        if count > 0 {
            self.chunk.emit(Op::DropLocals(count), pos);
            self.locals.truncate(outer);
        }
        Ok(())
    }

    fn local(&self, name: &str) -> Option<usize> {
        self.locals.iter().rposition(|n| n == name)
    }

    fn lookup(&self, name: &str) -> Option<Op> {
        if let Some(slot) = self.local(name) {
            return Some(Op::GetLocal(slot));
        }
        let index = self.functions.iter().position(|f| f.name == name)?;
        Some(Op::Function(index))
    }

    fn expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        let pos = expr.pos;
        match &expr.kind {
            ExprKind::Int(n) => {
                self.chunk.emit(Op::Int(*n), pos);
            }
            ExprKind::Bool(b) => {
                self.chunk.emit(Op::Bool(*b), pos);
            }
            ExprKind::Var(name) => {
                let op = self
                    .lookup(name)
                    .ok_or_else(|| CompileErrorKind::UndefinedVariable(name.clone()).at(pos))?;
                self.chunk.emit(op, pos);
            }
            ExprKind::Call { name, args } => {
                let callee = match self.lookup(name) {
                    Some(op) => op,
                    None if name == "print" => todo!("print in the VM"),
                    None => {
                        return Err(CompileErrorKind::UndefinedFunction(name.clone()).at(pos));
                    }
                };
                self.chunk.emit(callee, pos);
                self.temps += 1;
                for arg in args {
                    self.expr(arg)?;
                    self.temps += 1;
                }
                self.temps -= args.len() + 1;
                self.chunk.emit(Op::Call(args.len()), pos);
            }
            ExprKind::Unary { op, operand } => {
                self.expr(operand)?;
                self.chunk.emit(Op::Unary(op.clone()), pos);
            }
            ExprKind::Binary { op, left, right } => {
                self.expr(left)?;
                self.temps += 1;
                self.expr(right)?;
                self.temps -= 1;
                self.chunk.emit(Op::Binary(op.clone()), pos);
            }
            ExprKind::Logical { op, left, right } => self.logical(op, left, right, pos)?,
            ExprKind::Block(block) => self.block(block, pos)?,
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond)?;
                let to_else = self.chunk.emit(Op::JumpIfFalse(0), cond.pos);
                self.block(then_branch, pos)?;
                let to_end = self.chunk.emit(Op::Jump(0), pos);
                self.chunk.patch_jump(to_else);
                match else_branch {
                    Some(else_branch) => self.expr(else_branch)?,
                    None => {
                        self.chunk.emit(Op::Unit, pos);
                    }
                }
                self.chunk.patch_jump(to_end);
            }
            ExprKind::While { cond, body } => {
                let start = self.chunk.code.len();
                self.expr(cond)?;
                let to_exit = self.chunk.emit(Op::JumpIfFalse(0), cond.pos);
                self.loops.push(Loop {
                    start,
                    temps: self.temps,
                    locals: self.locals.len(),
                    breaks: Vec::new(),
                });
                self.block(body, pos)?;
                self.chunk.emit(Op::Pop, pos);
                self.chunk.emit(Op::Jump(start), pos);
                let innermost = self.loops.pop().expect("pushed above");
                self.chunk.patch_jump(to_exit);
                for jump in innermost.breaks {
                    self.chunk.patch_jump(jump);
                }
                self.chunk.emit(Op::Unit, pos);
            }
            ExprKind::Break => {
                self.leave_loop_body(pos);
                let jump = self.chunk.emit(Op::Jump(0), pos);
                self.innermost_loop().breaks.push(jump);
            }
            ExprKind::Continue => {
                self.leave_loop_body(pos);
                let start = self.innermost_loop().start;
                self.chunk.emit(Op::Jump(start), pos);
            }
            ExprKind::Return(value) => {
                match value {
                    Some(value) => self.expr(value)?,
                    None => {
                        self.chunk.emit(Op::Unit, pos);
                    }
                }
                self.chunk.emit(Op::Return, pos);
            }
        }
        Ok(())
    }

    fn innermost_loop(&mut self) -> &mut Loop {
        self.loops
            .last_mut()
            .expect("the parser rejects break and continue outside a loop")
    }

    fn leave_loop_body(&mut self, pos: Pos) {
        let innermost = self.innermost_loop();
        let (temps, locals) = (innermost.temps, innermost.locals);
        for _ in temps..self.temps {
            self.chunk.emit(Op::Pop, pos);
        }
        let count = self.locals.len() - locals;
        if count > 0 {
            self.chunk.emit(Op::DropLocals(count), pos);
        }
    }

    // Both operands must be bools: every operand goes through JumpIfFalse,
    // which reports a type error for anything else.
    fn logical(
        &mut self,
        op: &LogicalOp,
        left: &Expr,
        right: &Expr,
        pos: Pos,
    ) -> Result<(), CompileError> {
        let mut to_false = Vec::new();
        let mut to_true = Vec::new();

        self.expr(left)?;
        let left_false = self.chunk.emit(Op::JumpIfFalse(0), pos);
        match op {
            LogicalOp::And => to_false.push(left_false),
            LogicalOp::Or => {
                to_true.push(self.chunk.emit(Op::Jump(0), pos));
                self.chunk.patch_jump(left_false);
            }
        }

        self.expr(right)?;
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
        Ok(())
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
        compile(&program).unwrap().main.chunk.code.clone()
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
    fn top_level_let_defines_local() {
        assert_eq!(
            code("let x = 1; x = x + 2;"),
            [
                Op::Int(1),
                Op::DefineLocal,
                Op::GetLocal(0),
                Op::Int(2),
                Op::Binary(Add),
                Op::SetLocal(0)
            ]
        );
    }

    #[test]
    fn shadowing_gets_new_slot() {
        assert_eq!(
            code("let x = 1; let x = 2; x"),
            [
                Op::Int(1),
                Op::DefineLocal,
                Op::Int(2),
                Op::DefineLocal,
                Op::GetLocal(1)
            ]
        );
    }

    #[test]
    fn block_locals_follow_top_level_ones() {
        assert_eq!(
            code("let x = 1; { let y = x; y = 2; y }"),
            [
                Op::Int(1),
                Op::DefineLocal,
                Op::GetLocal(0),
                Op::DefineLocal,
                Op::Int(2),
                Op::SetLocal(1),
                Op::GetLocal(1),
                Op::DropLocals(1)
            ]
        );
    }

    #[test]
    fn nested_blocks_number_locals_from_outer() {
        assert_eq!(
            code("{ let a = 1; { let b = 2; a + b } }"),
            [
                Op::Int(1),
                Op::DefineLocal,
                Op::Int(2),
                Op::DefineLocal,
                Op::GetLocal(0),
                Op::GetLocal(1),
                Op::Binary(Add),
                Op::DropLocals(1),
                Op::DropLocals(1)
            ]
        );
    }

    #[test]
    fn while_jumps_back_to_condition() {
        assert_eq!(
            code("let x = true; while x { x = false; }"),
            [
                Op::Bool(true),
                Op::DefineLocal,
                Op::GetLocal(0),
                Op::JumpIfFalse(9),
                Op::Bool(false),
                Op::SetLocal(0),
                Op::Unit,
                Op::Pop,
                Op::Jump(2),
                Op::Unit,
            ]
        );
    }

    #[test]
    fn break_pops_temporaries() {
        assert_eq!(
            code("while true { 1 + break; }"),
            [
                Op::Bool(true),
                Op::JumpIfFalse(10),
                Op::Int(1),
                Op::Pop,
                Op::Jump(10),
                Op::Binary(Add),
                Op::Pop,
                Op::Unit,
                Op::Pop,
                Op::Jump(0),
                Op::Unit,
            ]
        );
    }

    #[test]
    fn continue_drops_locals() {
        assert_eq!(
            code("while true { let a = 1; continue; }"),
            [
                Op::Bool(true),
                Op::JumpIfFalse(11),
                Op::Int(1),
                Op::DefineLocal,
                Op::DropLocals(1),
                Op::Jump(0),
                Op::Pop,
                Op::Unit,
                Op::DropLocals(1),
                Op::Pop,
                Op::Jump(0),
                Op::Unit,
            ]
        );
    }

    #[test]
    fn call_pushes_callee_then_arguments() {
        assert_eq!(
            code("fn add(a, b) { a + b } add(1, 2)"),
            [Op::Function(0), Op::Int(1), Op::Int(2), Op::Call(2)]
        );
    }

    #[test]
    fn function_body_starts_with_parameters() {
        let program = Parser::new(tokenize("fn add(a, b) { let c = a + b; c }").unwrap())
            .parse()
            .unwrap();
        let bytecode = compile(&program).unwrap();
        assert_eq!(
            bytecode.functions[0].chunk.code,
            [
                Op::GetLocal(0),
                Op::GetLocal(1),
                Op::Binary(Add),
                Op::DefineLocal,
                Op::GetLocal(2),
                Op::DropLocals(1),
                Op::Return
            ]
        );
    }

    #[test]
    fn positions_follow_code() {
        let program = Parser::new(tokenize("1 +\n  2").unwrap()).parse().unwrap();
        let bytecode = compile(&program).unwrap();
        let positions: Vec<String> = bytecode
            .main
            .chunk
            .positions
            .iter()
            .map(|p| p.to_string())
            .collect();
        assert_eq!(positions, ["1:1", "2:3", "1:3"]);
    }
}
