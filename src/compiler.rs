use std::rc::Rc;

use crate::ast::{
    Access, BinaryOp, Block, Expr, ExprKind, FnDecl, Iterable, LogicalOp, Program, Stmt,
};
use crate::chunk::{Bytecode, Chunk, Function, Op};
use crate::lexer::Pos;
use crate::value::StructDef;

#[derive(Debug, Clone, Default)]
pub struct Scope {
    locals: Vec<String>,
    functions: Vec<String>,
    structs: Vec<Rc<StructDef>>,
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
    let mut structs = scope.structs.clone();
    structs.extend(program.stmts.iter().filter_map(|stmt| match stmt {
        Stmt::Struct(decl) => Some(Rc::new(StructDef {
            name: decl.name.clone(),
            fields: decl.fields.iter().map(|f| f.name.clone()).collect(),
        })),
        _ => None,
    }));
    let functions = decls
        .iter()
        .map(|decl| function(decl, &names, &structs))
        .collect();

    let mut compiler = Compiler::new(&names, &structs, scope.locals.clone());
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
        structs,
    };
    Bytecode {
        main: Rc::new(main),
        functions,
        scope,
    }
}

fn function(decl: &FnDecl, names: &[String], structs: &[Rc<StructDef>]) -> Rc<Function> {
    let params = decl.params.iter().map(|p| p.name.clone()).collect();
    let mut compiler = Compiler::new(names, structs, params);
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
    structs: &'a [Rc<StructDef>],
    chunk: Chunk,
    locals: Vec<String>,
    temps: usize,
    loops: Vec<Loop>,
}

struct Loop {
    // Where both `continue` and the end of the body jump.
    start: usize,
    temps: usize,
    locals: usize,
    breaks: Vec<usize>,
}

impl<'a> Compiler<'a> {
    fn new(functions: &'a [String], structs: &'a [Rc<StructDef>], locals: Vec<String>) -> Self {
        Self {
            functions,
            structs,
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
                    path,
                    op: Some((op, op_pos)),
                    value,
                    pos,
                } => self.compound(name, path, op, *op_pos, value, *pos),
                Stmt::Assign {
                    name,
                    path,
                    op: None,
                    value,
                    pos,
                } => {
                    self.expr(value);
                    self.temps += 1;
                    self.place_indexes(path);
                    self.temps -= path.len() + 1;
                    let slot = self.local(name).expect(RESOLVED);
                    let op = match path.len() {
                        0 => Op::SetLocal(slot),
                        depth => Op::SetIndex { slot, depth },
                    };
                    self.chunk.emit(op, *pos);
                }
                Stmt::Expr(expr) => {
                    self.expr(expr);
                    self.chunk.emit(Op::Pop, expr.pos);
                }
                Stmt::Fn(_) | Stmt::Struct(_) => {}
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
            ExprKind::Struct { name, fields } => {
                let def = self
                    .structs
                    .iter()
                    .find(|def| &def.name == name)
                    .expect(RESOLVED);
                let mut places = Vec::new();
                for (field, _, value) in fields {
                    self.expr(value);
                    self.temps += 1;
                    let place = def.fields.iter().position(|f| f == field);
                    places.push(place.expect(RESOLVED));
                }
                self.temps -= fields.len();
                let op = Op::Struct {
                    def: Rc::clone(def),
                    fields: places,
                };
                self.chunk.emit(op, pos);
            }
            ExprKind::Access { target, path } => {
                self.expr(target);
                for (pos, step) in path {
                    self.temps += 1;
                    let op = match step {
                        Access::Index(index) => {
                            self.expr(index);
                            Op::Index
                        }
                        Access::Slice { start, end } => {
                            let bounds: Vec<&Expr> = step.exprs().collect();
                            for bound in &bounds {
                                self.expr(bound);
                                self.temps += 1;
                            }
                            self.temps -= bounds.len();
                            Op::Slice {
                                start: start.is_some(),
                                end: end.is_some(),
                            }
                        }
                    };
                    self.temps -= 1;
                    self.chunk.emit(op, *pos);
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
            ExprKind::For {
                var, iter, body, ..
            } => self.for_loop(var, iter, body, pos),
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

    // Runs as `let counter = start; let end = end;` and then
    // `while counter < end { let var = counter; body; counter = counter + 1; }`.
    // An array goes into a local before them, the counter runs from 0 to its
    // length, and `var` is `array[counter]`.
    // These locals have no name, so no program can reach them. The increment
    // comes before the condition and is skipped on entry, so `continue` jumps
    // back to it, as in `while`. It runs only after `counter < end`, so it
    // cannot overflow.
    fn for_loop(&mut self, var: &str, iter: &Iterable, body: &Block, pos: Pos) {
        let outer = self.locals.len();
        let array = match iter {
            Iterable::Range { start, end } => {
                self.expr(start);
                self.define_hidden(start.pos);
                self.expr(end);
                self.define_hidden(end.pos);
                None
            }
            Iterable::Array(array) => {
                self.expr(array);
                self.define_hidden(array.pos);
                self.chunk.emit(Op::Int(0), pos);
                self.define_hidden(pos);
                self.chunk.emit(Op::GetLocal(outer), pos);
                self.chunk.emit(Op::Len, pos);
                self.define_hidden(pos);
                Some(outer)
            }
        };
        let counter = self.locals.len() - 2;
        let to_cond = self.chunk.emit(Op::Jump(0), pos);
        let next = self.chunk.code.len();
        self.chunk.emit(Op::GetLocal(counter), pos);
        self.chunk.emit(Op::Int(1), pos);
        self.chunk.emit(Op::Binary(BinaryOp::Add), pos);
        self.chunk.emit(Op::SetLocal(counter), pos);
        self.chunk.patch_jump(to_cond);
        self.chunk.emit(Op::GetLocal(counter), pos);
        self.chunk.emit(Op::GetLocal(counter + 1), pos);
        self.chunk.emit(Op::Binary(BinaryOp::Lt), pos);
        let to_exit = self.chunk.emit(Op::JumpIfFalse(0), pos);
        self.loops.push(Loop {
            start: next,
            temps: self.temps,
            locals: self.locals.len(),
            breaks: Vec::new(),
        });
        match array {
            Some(array) => {
                self.chunk.emit(Op::GetLocal(array), pos);
                self.chunk.emit(Op::GetLocal(counter), pos);
                self.chunk.emit(Op::Index, pos);
            }
            None => {
                self.chunk.emit(Op::GetLocal(counter), pos);
            }
        }
        self.chunk.emit(Op::DefineLocal, pos);
        self.locals.push(var.to_string());
        self.block(body, pos);
        self.chunk.emit(Op::Pop, pos);
        self.chunk.emit(Op::DropLocals(1), pos);
        self.locals.pop();
        self.chunk.emit(Op::Jump(next), pos);
        let innermost = self.loops.pop().expect("pushed above");
        self.chunk.patch_jump(to_exit);
        for jump in innermost.breaks {
            self.chunk.patch_jump(jump);
        }
        let count = self.locals.len() - outer;
        self.chunk.emit(Op::DropLocals(count), pos);
        self.locals.truncate(outer);
        self.chunk.emit(Op::Unit, pos);
    }

    fn define_hidden(&mut self, pos: Pos) {
        self.chunk.emit(Op::DefineLocal, pos);
        self.locals.push(String::new());
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
            "bytes" => Op::Bytes,
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
        self.place_indexes(place.path);
        self.expr(value);
        let depth = place.path.len();
        self.temps -= depth;
        let slot = self.local(place.name).expect(RESOLVED);
        self.chunk.emit(Op::Push { slot, depth }, pos);
    }

    // Leaves the indexes on the stack, counted in `temps`.
    fn place_indexes(&mut self, path: &[(Pos, Access)]) {
        for (_, step) in path {
            let Access::Index(index) = step else {
                panic!("the checker rejects assignment to a slice");
            };
            self.expr(index);
            self.temps += 1;
        }
    }

    // The place is read before the value is evaluated. Each index is
    // evaluated once and kept in an unnamed local for both the read and the
    // write.
    fn compound(
        &mut self,
        name: &str,
        path: &[(Pos, Access)],
        op: &BinaryOp,
        op_pos: Pos,
        value: &Expr,
        pos: Pos,
    ) {
        for (bracket, step) in path {
            let Access::Index(index) = step else {
                panic!("the checker rejects assignment to a slice");
            };
            self.expr(index);
            self.chunk.emit(Op::DefineLocal, *bracket);
            self.locals.push(String::new());
        }
        let first = self.locals.len() - path.len();
        let slot = self.local(name).expect(RESOLVED);
        self.chunk.emit(Op::GetLocal(slot), pos);
        for (i, (bracket, _)) in path.iter().enumerate() {
            self.chunk.emit(Op::GetLocal(first + i), *bracket);
            self.chunk.emit(Op::Index, *bracket);
        }
        self.temps += 1;
        self.expr(value);
        self.temps -= 1;
        self.chunk.emit(Op::Binary(op.clone()), op_pos);
        if path.is_empty() {
            self.chunk.emit(Op::SetLocal(slot), pos);
            return;
        }
        for i in 0..path.len() {
            self.chunk.emit(Op::GetLocal(first + i), pos);
        }
        let depth = path.len();
        self.chunk.emit(Op::SetIndex { slot, depth }, pos);
        self.chunk.emit(Op::DropLocals(depth), pos);
        self.locals.truncate(first);
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
    use crate::ast::BinaryOp::{Add, Lt, Mul};
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
    fn for_keeps_counter_and_end_in_locals() {
        assert_eq!(
            code("for _ in 0..3 {}"),
            [
                Op::Int(0),
                Op::DefineLocal,
                Op::Int(3),
                Op::DefineLocal,
                Op::Jump(9),
                Op::GetLocal(0),
                Op::Int(1),
                Op::Binary(Add),
                Op::SetLocal(0),
                Op::GetLocal(0),
                Op::GetLocal(1),
                Op::Binary(Lt),
                Op::JumpIfFalse(19),
                Op::GetLocal(0),
                Op::DefineLocal,
                Op::Unit,
                Op::Pop,
                Op::DropLocals(1),
                Op::Jump(5),
                Op::DropLocals(2),
                Op::Unit,
            ]
        );
    }

    #[test]
    fn continue_in_for_jumps_to_increment() {
        assert_eq!(
            code("for _ in 0..3 { continue; }"),
            [
                Op::Int(0),
                Op::DefineLocal,
                Op::Int(3),
                Op::DefineLocal,
                Op::Jump(9),
                Op::GetLocal(0),
                Op::Int(1),
                Op::Binary(Add),
                Op::SetLocal(0),
                Op::GetLocal(0),
                Op::GetLocal(1),
                Op::Binary(Lt),
                Op::JumpIfFalse(22),
                Op::GetLocal(0),
                Op::DefineLocal,
                Op::DropLocals(1),
                Op::Jump(5),
                Op::Pop,
                Op::Unit,
                Op::Pop,
                Op::DropLocals(1),
                Op::Jump(5),
                Op::DropLocals(2),
                Op::Unit,
            ]
        );
    }

    #[test]
    fn for_over_array_indexes_a_hidden_copy() {
        assert_eq!(
            code("for _ in [7] {}"),
            [
                Op::Int(7),
                Op::Array(1),
                Op::DefineLocal,
                Op::Int(0),
                Op::DefineLocal,
                Op::GetLocal(0),
                Op::Len,
                Op::DefineLocal,
                Op::Jump(13),
                Op::GetLocal(1),
                Op::Int(1),
                Op::Binary(Add),
                Op::SetLocal(1),
                Op::GetLocal(1),
                Op::GetLocal(2),
                Op::Binary(Lt),
                Op::JumpIfFalse(25),
                Op::GetLocal(0),
                Op::GetLocal(1),
                Op::Index,
                Op::DefineLocal,
                Op::Unit,
                Op::Pop,
                Op::DropLocals(1),
                Op::Jump(9),
                Op::DropLocals(3),
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
    fn struct_literal_keeps_written_order() {
        let def = Rc::new(StructDef {
            name: "P".to_string(),
            fields: vec!["x".to_string(), "y".to_string()],
        });
        assert_eq!(
            code("struct P { x: int, y: int } P { y: 2, x: 1 }"),
            [
                Op::Int(2),
                Op::Int(1),
                Op::Struct {
                    def,
                    fields: vec![1, 0]
                }
            ]
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
    fn compound_assignment_reads_then_writes() {
        assert_eq!(
            code("let mut x = 1; x += 2;"),
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
    fn compound_index_assignment_keeps_indexes_in_locals() {
        assert_eq!(
            code("let mut a = [[1]]; a[0][1] *= 2;"),
            [
                Op::Int(1),
                Op::Array(1),
                Op::Array(1),
                Op::DefineLocal,
                Op::Int(0),
                Op::DefineLocal,
                Op::Int(1),
                Op::DefineLocal,
                Op::GetLocal(0),
                Op::GetLocal(1),
                Op::Index,
                Op::GetLocal(2),
                Op::Index,
                Op::Int(2),
                Op::Binary(Mul),
                Op::GetLocal(1),
                Op::GetLocal(2),
                Op::SetIndex { slot: 0, depth: 2 },
                Op::DropLocals(2)
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
    fn slice_pushes_only_given_bounds() {
        assert_eq!(
            code(r#""abc"[1..][..2][..]"#),
            [
                Op::Str("abc".into()),
                Op::Int(1),
                Op::Slice {
                    start: true,
                    end: false
                },
                Op::Int(2),
                Op::Slice {
                    start: false,
                    end: true
                },
                Op::Slice {
                    start: false,
                    end: false
                }
            ]
        );
    }

    #[test]
    fn len_compiles_to_instruction() {
        assert_eq!(code(r#"len("a")"#), [Op::Str("a".into()), Op::Len]);
    }

    #[test]
    fn bytes_compiles_to_instruction() {
        assert_eq!(code(r#"bytes("a")"#), [Op::Str("a".into()), Op::Bytes]);
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
