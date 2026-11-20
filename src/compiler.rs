use std::rc::Rc;

use crate::ast::{Block, Expr, ExprKind, FnDecl, LogicalOp, Program, Stmt};
use crate::chunk::{Bytecode, Chunk, Function, Op};
use crate::lexer::Pos;

#[derive(Debug, Clone, Default)]
pub struct Scope {
    locals: Vec<String>,
    functions: Vec<String>,
}

pub fn compile(program: &Program, scope: &Scope) -> Bytecode {
    let decls: Vec<&FnDecl> = program
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Fn(decl) => Some(decl),
            _ => None,
        })
        .collect();
    let mut names = scope.functions.clone();
    names.extend(decls.iter().map(|decl| decl.name.clone()));
    let functions = decls.iter().map(|decl| function(decl, &names)).collect();

    let mut compiler = Compiler::new(&names, scope.locals.clone());
    compiler.stmts(&program.stmts);
    if let Some(expr) = &program.expr {
        compiler.expr(expr);
    }
    let main = Function {
        name: "main".to_string(),
        chunk: compiler.chunk,
    };
    let scope = Scope {
        locals: compiler.locals,
        functions: names,
    };
    Bytecode {
        main: Rc::new(main),
        functions,
        scope,
    }
}

fn function(decl: &FnDecl, names: &[String]) -> Rc<Function> {
    let params = decl.params.iter().map(|p| p.name.clone()).collect();
    let mut compiler = Compiler::new(names, params);
    compiler.block(&decl.body, decl.pos);
    compiler.chunk.emit(Op::Return, decl.pos);
    Rc::new(Function {
        name: decl.name.clone(),
        chunk: compiler.chunk,
    })
}

const RESOLVED: &str = "the checker resolves every name";

struct Compiler<'a> {
    functions: &'a [String],
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
    fn new(functions: &'a [String], locals: Vec<String>) -> Self {
        Self {
            functions,
            chunk: Chunk::default(),
            locals,
            temps: 0,
            loops: Vec::new(),
        }
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, value, .. } => {
                    self.expr(value);
                    self.chunk.emit(Op::DefineLocal, value.pos);
                    self.locals.push(name.clone());
                }
                Stmt::Assign {
                    name,
                    indexes,
                    value,
                    pos,
                } => {
                    self.expr(value);
                    self.temps += 1;
                    for (_, index) in indexes {
                        self.expr(index);
                        self.temps += 1;
                    }
                    self.temps -= indexes.len() + 1;
                    let slot = self.local(name).expect(RESOLVED);
                    let op = match indexes.len() {
                        0 => Op::SetLocal(slot),
                        depth => Op::SetIndex { slot, depth },
                    };
                    self.chunk.emit(op, *pos);
                }
                Stmt::Expr(expr) => {
                    self.expr(expr);
                    self.chunk.emit(Op::Pop, expr.pos);
                }
                Stmt::Fn(_) => {}
            }
        }
    }

    fn block(&mut self, block: &Block, pos: Pos) {
        let outer = self.locals.len();
        self.stmts(&block.stmts);
        match &block.expr {
            Some(value) => self.expr(value),
            None => {
                self.chunk.emit(Op::Unit, pos);
            }
        }
        let count = self.locals.len() - outer;
        if count > 0 {
            self.chunk.emit(Op::DropLocals(count), pos);
            self.locals.truncate(outer);
        }
    }

    fn local(&self, name: &str) -> Option<usize> {
        self.locals.iter().rposition(|n| n == name)
    }

    fn lookup(&self, name: &str) -> Option<Op> {
        if let Some(slot) = self.local(name) {
            return Some(Op::GetLocal(slot));
        }
        let index = self.functions.iter().rposition(|f| f == name)?;
        Some(Op::Function(index))
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
            ExprKind::Str(s) => {
                self.chunk.emit(Op::Str(Rc::from(s.as_str())), pos);
            }
            ExprKind::Var(name) => {
                let op = self.lookup(name).expect(RESOLVED);
                self.chunk.emit(op, pos);
            }
            ExprKind::Call { name, args } => {
                // Only built-in functions pass the checker without a declaration.
                let Some(callee) = self.lookup(name) else {
                    return self.builtin(name, args, pos);
                };
                self.chunk.emit(callee, pos);
                self.temps += 1;
                for arg in args {
                    self.expr(arg);
                    self.temps += 1;
                }
                self.temps -= args.len() + 1;
                self.chunk.emit(Op::Call(args.len()), pos);
            }
            ExprKind::Unary { op, operand } => {
                self.expr(operand);
                self.chunk.emit(Op::Unary(op.clone()), pos);
            }
            ExprKind::Binary { left, rest } => {
                self.expr(left);
                for (op, pos, right) in rest {
                    self.temps += 1;
                    self.expr(right);
                    self.temps -= 1;
                    self.chunk.emit(Op::Binary(op.clone()), *pos);
                }
            }
            ExprKind::Logical { left, rest } => {
                self.expr(left);
                for (op, pos, right) in rest {
                    self.logical(op, right, *pos);
                }
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.expr(item);
                    self.temps += 1;
                }
                self.temps -= items.len();
                self.chunk.emit(Op::Array(items.len()), pos);
            }
            ExprKind::Index { target, indexes } => {
                self.expr(target);
                for (pos, index) in indexes {
                    self.temps += 1;
                    self.expr(index);
                    self.temps -= 1;
                    self.chunk.emit(Op::Index, *pos);
                }
            }
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
            ExprKind::While { cond, body } => {
                let start = self.chunk.code.len();
                self.expr(cond);
                let to_exit = self.chunk.emit(Op::JumpIfFalse(0), cond.pos);
                self.loops.push(Loop {
                    start,
                    temps: self.temps,
                    locals: self.locals.len(),
                    breaks: Vec::new(),
                });
                self.block(body, pos);
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
                    Some(value) => self.expr(value),
                    None => {
                        self.chunk.emit(Op::Unit, pos);
                    }
                }
                self.chunk.emit(Op::Return, pos);
            }
        }
    }

    fn builtin(&mut self, name: &str, args: &[Expr], pos: Pos) {
        if name == "push" {
            return self.push(args, pos);
        }
        let [value] = args else {
            panic!("the checker gives built-in functions exactly one argument");
        };
        let op = match name {
            "print" => Op::Print,
            "len" => Op::Len,
            _ => panic!("the checker rejects undefined function {name}"),
        };
        self.expr(value);
        self.chunk.emit(op, pos);
    }

    fn push(&mut self, args: &[Expr], pos: Pos) {
        let [target, value] = args else {
            panic!("the checker gives push two arguments");
        };
        let place = target
            .as_place()
            .expect("the checker only lets push change a variable");
        for (_, index) in place.indexes {
            self.expr(index);
            self.temps += 1;
        }
        self.expr(value);
        let depth = place.indexes.len();
        self.temps -= depth;
        let slot = self.local(place.name).expect(RESOLVED);
        self.chunk.emit(Op::Push { slot, depth }, pos);
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

    // The left operand is already on the stack. The checker ensures both
    // operands are bools, so every operand can go through `JumpIfFalse`.
    fn logical(&mut self, op: &LogicalOp, right: &Expr, pos: Pos) {
        let mut to_false = Vec::new();
        let mut to_true = Vec::new();

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
    use crate::checker::{Env, check};
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn code(input: &str) -> Vec<Op> {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        assert_eq!(check(&program, &Env::default()).errors, []);
        compile(&program, &Scope::default()).main.chunk.code.clone()
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
            code("if true { 1; }"),
            [
                Op::Bool(true),
                Op::JumpIfFalse(6),
                Op::Int(1),
                Op::Pop,
                Op::Unit,
                Op::Jump(7),
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
            code("let mut x = 1; x = x + 2;"),
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
            code("let x = 1; { let mut y = x; y = 2; y }"),
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
            code("let mut x = true; while x { x = false; }"),
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
            code("fn add(a: int, b: int) -> int { a + b } add(1, 2)"),
            [Op::Function(0), Op::Int(1), Op::Int(2), Op::Call(2)]
        );
    }

    #[test]
    fn function_body_starts_with_parameters() {
        let program =
            Parser::new(tokenize("fn add(a: int, b: int) -> int { let c = a + b; c }").unwrap())
                .parse()
                .unwrap();
        let bytecode = compile(&program, &Scope::default());
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
    fn print_compiles_to_instruction() {
        assert_eq!(code("print(1)"), [Op::Int(1), Op::Print]);
    }

    #[test]
    fn array_and_index() {
        assert_eq!(
            code("[1, 2][0]"),
            [Op::Int(1), Op::Int(2), Op::Array(2), Op::Int(0), Op::Index]
        );
    }

    #[test]
    fn index_assignment() {
        assert_eq!(
            code("let mut a = [[1]]; a[0][0] = 2;"),
            [
                Op::Int(1),
                Op::Array(1),
                Op::Array(1),
                Op::DefineLocal,
                Op::Int(2),
                Op::Int(0),
                Op::Int(0),
                Op::SetIndex { slot: 0, depth: 2 }
            ]
        );
    }

    #[test]
    fn push_compiles_to_instruction() {
        assert_eq!(
            code("let mut a = [[1]]; push(a[0], 2);"),
            [
                Op::Int(1),
                Op::Array(1),
                Op::Array(1),
                Op::DefineLocal,
                Op::Int(0),
                Op::Int(2),
                Op::Push { slot: 0, depth: 1 },
                Op::Pop
            ]
        );
    }

    #[test]
    fn len_compiles_to_instruction() {
        assert_eq!(code(r#"len("a")"#), [Op::Str("a".into()), Op::Len]);
    }

    #[test]
    fn string_is_a_constant() {
        assert_eq!(
            code(r#""a" + "b""#),
            [Op::Str("a".into()), Op::Str("b".into()), Op::Binary(Add)]
        );
    }

    #[test]
    fn positions_follow_code() {
        let program = Parser::new(tokenize("1 +\n  2").unwrap()).parse().unwrap();
        let bytecode = compile(&program, &Scope::default());
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
