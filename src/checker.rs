use std::fmt;

use crate::ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, Program, Stmt, Type, UnaryOp};
use crate::lexer::Pos;

#[derive(Debug, Clone, PartialEq)]
pub struct CheckError {
    pub kind: CheckErrorKind,
    pub pos: Pos,
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CheckErrorKind {
    UndefinedVariable(String),
    UndefinedFunction(String),
    DuplicateFunction(String),
    PrintArgumentCount(usize),
    Mismatch {
        expected: Type,
        found: Type,
    },
    NotAFunction(Type),
    ArgumentCount {
        name: String,
        expected: usize,
        found: usize,
    },
    CannotCompare(Type),
}

impl CheckErrorKind {
    fn at(self, pos: Pos) -> CheckError {
        CheckError { kind: self, pos }
    }
}

impl fmt::Display for CheckErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CheckErrorKind::UndefinedVariable(name) => write!(f, "undefined variable: {name}"),
            CheckErrorKind::UndefinedFunction(name) => write!(f, "undefined function: {name}"),
            CheckErrorKind::DuplicateFunction(name) => write!(f, "duplicate function: {name}"),
            CheckErrorKind::PrintArgumentCount(found) => {
                write!(f, "print expects 1 argument, got {found}")
            }
            CheckErrorKind::Mismatch { expected, found } => {
                write!(f, "expected {expected}, found {found}")
            }
            CheckErrorKind::NotAFunction(found) => write!(f, "expected function, found {found}"),
            CheckErrorKind::ArgumentCount {
                name,
                expected,
                found,
            } => {
                let s = if *expected == 1 { "" } else { "s" };
                write!(f, "{name} expects {expected} argument{s}, got {found}")
            }
            CheckErrorKind::CannotCompare(ty) => write!(f, "cannot compare values of type {ty}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env {
    locals: Vec<(String, Type)>,
    functions: Vec<(String, Type)>,
}

pub fn check(program: &Program, env: &Env) -> Result<Env, CheckError> {
    let decls: Vec<&FnDecl> = program
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Fn(decl) => Some(decl),
            _ => None,
        })
        .collect();
    for (i, decl) in decls.iter().enumerate() {
        if decls[..i].iter().any(|other| other.name == decl.name) {
            return Err(CheckErrorKind::DuplicateFunction(decl.name.clone()).at(decl.pos));
        }
    }

    let mut functions = env.functions.clone();
    functions.extend(decls.iter().map(|decl| (decl.name.clone(), decl.ty())));
    for decl in &decls {
        function(decl, &functions)?;
    }

    let mut checker = Checker::new(&functions, env.locals.clone(), Type::Unit);
    checker.stmts(&program.stmts)?;
    if let Some(expr) = &program.expr {
        checker.expr(expr)?;
    }
    Ok(Env {
        locals: checker.locals,
        functions,
    })
}

fn function(decl: &FnDecl, functions: &[(String, Type)]) -> Result<(), CheckError> {
    let params = decl
        .params
        .iter()
        .map(|p| (p.name.clone(), p.ty.clone()))
        .collect();
    let mut checker = Checker::new(functions, params, decl.ret.clone());
    let body = checker.block(&decl.body)?;
    expect(&decl.ret, &body, block_value_pos(&decl.body, decl.pos))
}

fn value_pos(expr: &Expr) -> Pos {
    match &expr.kind {
        ExprKind::Block(block) => block_value_pos(block, expr.pos),
        _ => expr.pos,
    }
}

fn block_value_pos(block: &Block, fallback: Pos) -> Pos {
    block.expr.as_deref().map_or(fallback, value_pos)
}

fn expect(expected: &Type, found: &Type, pos: Pos) -> Result<(), CheckError> {
    if found == expected || *found == Type::Never {
        Ok(())
    } else {
        Err(CheckErrorKind::Mismatch {
            expected: expected.clone(),
            found: found.clone(),
        }
        .at(pos))
    }
}

fn join(first: Type, second: Type, pos: Pos) -> Result<Type, CheckError> {
    if first == Type::Never {
        return Ok(second);
    }
    expect(&first, &second, pos)?;
    Ok(first)
}

struct Checker<'a> {
    functions: &'a [(String, Type)],
    locals: Vec<(String, Type)>,
    ret: Type,
    diverges: bool,
}

impl<'a> Checker<'a> {
    fn new(functions: &'a [(String, Type)], locals: Vec<(String, Type)>, ret: Type) -> Self {
        Self {
            functions,
            locals,
            ret,
            diverges: false,
        }
    }

    fn stmts(&mut self, stmts: &[Stmt]) -> Result<(), CheckError> {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, value } => {
                    let ty = self.expr(value)?;
                    self.locals.push((name.clone(), ty));
                }
                Stmt::Assign { name, value, pos } => {
                    let found = self.expr(value)?;
                    let expected = self
                        .local(name)
                        .ok_or_else(|| CheckErrorKind::UndefinedVariable(name.clone()).at(*pos))?;
                    expect(&expected, &found, value_pos(value))?;
                }
                Stmt::Expr(expr) => {
                    self.expr(expr)?;
                }
                Stmt::Fn(_) => {}
            }
        }
        Ok(())
    }

    fn block(&mut self, block: &Block) -> Result<Type, CheckError> {
        let outer = self.locals.len();
        self.stmts(&block.stmts)?;
        let ty = match &block.expr {
            Some(value) => self.expr(value)?,
            None if self.diverges => Type::Never,
            None => Type::Unit,
        };
        self.locals.truncate(outer);
        Ok(ty)
    }

    fn local(&self, name: &str) -> Option<Type> {
        let (_, ty) = self.locals.iter().rfind(|(n, _)| n == name)?;
        Some(ty.clone())
    }

    fn lookup(&self, name: &str) -> Option<Type> {
        if let Some(ty) = self.local(name) {
            return Some(ty);
        }
        let (_, ty) = self.functions.iter().rfind(|(n, _)| n == name)?;
        Some(ty.clone())
    }

    fn expr(&mut self, expr: &Expr) -> Result<Type, CheckError> {
        let pos = expr.pos;
        let ty = match &expr.kind {
            ExprKind::Int(_) => Type::Int,
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Var(name) => self
                .lookup(name)
                .ok_or_else(|| CheckErrorKind::UndefinedVariable(name.clone()).at(pos))?,
            ExprKind::Call { name, args } => {
                let callee = match self.lookup(name) {
                    Some(ty) => ty,
                    None if name == "print" => return self.print(args, pos),
                    None => return Err(CheckErrorKind::UndefinedFunction(name.clone()).at(pos)),
                };
                let Type::Fn { params, ret } = callee else {
                    return Err(CheckErrorKind::NotAFunction(callee).at(pos));
                };
                if args.len() != params.len() {
                    return Err(CheckErrorKind::ArgumentCount {
                        name: name.clone(),
                        expected: params.len(),
                        found: args.len(),
                    }
                    .at(pos));
                }
                for (arg, param) in args.iter().zip(&params) {
                    let found = self.expr(arg)?;
                    expect(param, &found, value_pos(arg))?;
                }
                *ret
            }
            ExprKind::Unary { op, operand } => {
                let ty = match op {
                    UnaryOp::Neg => Type::Int,
                    UnaryOp::Not => Type::Bool,
                };
                let found = self.expr(operand)?;
                expect(&ty, &found, value_pos(operand))?;
                ty
            }
            ExprKind::Binary { op, left, right } => self.binary(op, left, right, pos)?,
            ExprKind::Logical { left, right, .. } => {
                let found = self.expr(left)?;
                expect(&Type::Bool, &found, value_pos(left))?;
                let diverges = self.diverges;
                let found = self.expr(right)?;
                expect(&Type::Bool, &found, value_pos(right))?;
                self.diverges = diverges;
                Type::Bool
            }
            ExprKind::Block(block) => self.block(block)?,
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let found = self.expr(cond)?;
                expect(&Type::Bool, &found, value_pos(cond))?;
                let diverges = self.diverges;
                let then_ty = self.block(then_branch)?;
                match else_branch {
                    Some(else_branch) => {
                        let then_diverges = self.diverges;
                        self.diverges = diverges;
                        let else_ty = self.expr(else_branch)?;
                        self.diverges = then_diverges && self.diverges;
                        join(then_ty, else_ty, value_pos(else_branch))?
                    }
                    None => {
                        expect(&Type::Unit, &then_ty, block_value_pos(then_branch, pos))?;
                        self.diverges = diverges;
                        Type::Unit
                    }
                }
            }
            ExprKind::While { cond, body } => {
                let found = self.expr(cond)?;
                expect(&Type::Bool, &found, value_pos(cond))?;
                let diverges = self.diverges;
                let body_ty = self.block(body)?;
                expect(&Type::Unit, &body_ty, block_value_pos(body, pos))?;
                self.diverges = diverges;
                Type::Unit
            }
            ExprKind::Return(value) => {
                let (found, pos) = match value {
                    Some(value) => (self.expr(value)?, value_pos(value)),
                    None => (Type::Unit, pos),
                };
                expect(&self.ret, &found, pos)?;
                self.diverges = true;
                Type::Never
            }
            ExprKind::Break | ExprKind::Continue => {
                self.diverges = true;
                Type::Never
            }
        };
        Ok(ty)
    }

    fn binary(
        &mut self,
        op: &BinaryOp,
        left: &Expr,
        right: &Expr,
        pos: Pos,
    ) -> Result<Type, CheckError> {
        let left_ty = self.expr(left)?;
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let right_ty = self.expr(right)?;
            let ty = join(left_ty, right_ty, value_pos(right))?;
            if matches!(ty, Type::Fn { .. }) {
                return Err(CheckErrorKind::CannotCompare(ty).at(pos));
            }
            return Ok(Type::Bool);
        }
        expect(&Type::Int, &left_ty, value_pos(left))?;
        let right_ty = self.expr(right)?;
        expect(&Type::Int, &right_ty, value_pos(right))?;
        Ok(match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => Type::Int,
            _ => Type::Bool,
        })
    }

    fn print(&mut self, args: &[Expr], pos: Pos) -> Result<Type, CheckError> {
        let [value] = args else {
            return Err(CheckErrorKind::PrintArgumentCount(args.len()).at(pos));
        };
        self.expr(value)?;
        Ok(Type::Unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn check_program(input: &str) -> Result<Env, CheckError> {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        check(&program, &Env::default())
    }

    fn error(input: &str) -> String {
        check_program(input).unwrap_err().to_string()
    }

    #[test]
    fn well_typed_programs() {
        for input in [
            "(1 + 2 * 3 < 7) == true",
            "let x = 1; x = x + 1; x",
            "if true { 1 } else { 2 }",
            "if true { print(1); }",
            "print(1) == print(2)",
            "let i = 0; while i < 3 { i = i + 1; if i == 2 { break; } }",
            "fn f() {} let g = f; g()",
            "fn print(x: bool) -> bool { x } print(true) && true",
            "fn twice(f: fn(int) -> int, x: int) -> int { f(f(x)) }
             fn inc(x: int) -> int { x + 1 }
             twice(inc, 1)",
            "fn even(n: int) -> bool { if n == 0 { true } else { odd(n - 1) } }
             fn odd(n: int) -> bool { if n == 0 { false } else { even(n - 1) } }",
        ] {
            assert!(check_program(input).is_ok(), "{input}");
        }
    }

    #[test]
    fn return_break_and_continue_fit_any_type() {
        for input in [
            "fn f(n: int) -> int { if n < 0 { return 0 } else { n } }",
            "fn f() -> int { return 1; }",
            "fn f() -> int { 1 + return 2; }",
            "fn f() -> int { { { return 1; } } }",
            "fn f() -> int { while true { return 1; } 0 }",
            "while true { 1 + break; }",
            "while true { let x = if true { continue } else { 1 }; }",
        ] {
            assert!(check_program(input).is_ok(), "{input}");
        }
    }

    #[test]
    fn undefined_variables() {
        assert_eq!(error("x"), "1:1: undefined variable: x");
        assert_eq!(error("y = 1;"), "1:1: undefined variable: y");
        assert_eq!(error("{ let a = 1; } a"), "1:16: undefined variable: a");
        assert_eq!(error("let x = x;"), "1:9: undefined variable: x");
        assert_eq!(
            error("let y = x; let x = 1; y"),
            "1:9: undefined variable: x"
        );
        assert_eq!(error("if false { y }"), "1:12: undefined variable: y");
    }

    #[test]
    fn functions_do_not_see_outer_variables() {
        assert_eq!(
            error("let base = 10; fn f(x: int) -> int { base + x } f(1)"),
            "1:38: undefined variable: base"
        );
        assert_eq!(
            error("fn f() -> int { y } { let y = 1; f() }"),
            "1:17: undefined variable: y"
        );
        assert_eq!(
            error("let count = 0; fn reset() { count = 0; }"),
            "1:29: undefined variable: count"
        );
    }

    #[test]
    fn function_name_errors() {
        assert_eq!(error("f()"), "1:1: undefined function: f");
        assert_eq!(error("fn f() {} fn f() {}"), "1:14: duplicate function: f");
        assert_eq!(error("fn f() {} f = 1;"), "1:11: undefined variable: f");
        assert_eq!(error("print(1, 2)"), "1:1: print expects 1 argument, got 2");
    }

    #[test]
    fn operand_types() {
        assert_eq!(error("1 +\n  true"), "2:3: expected int, found bool");
        assert_eq!(error("-false"), "1:2: expected int, found bool");
        assert_eq!(error("!1"), "1:2: expected bool, found int");
        assert_eq!(error("false < true"), "1:1: expected int, found bool");
        assert_eq!(error("true && 1"), "1:9: expected bool, found int");
        assert_eq!(error("1 || true"), "1:1: expected bool, found int");
        assert_eq!(error("print(1) + 1"), "1:1: expected int, found ()");
        assert_eq!(
            error("let x = true;\nx + 1"),
            "2:1: expected int, found bool"
        );
        assert_eq!(error("1 + { 2; true }"), "1:10: expected int, found bool");
    }

    #[test]
    fn comparison_types() {
        assert_eq!(error("1 == true"), "1:6: expected int, found bool");
        assert_eq!(
            error("fn f() {} f == f"),
            "1:13: cannot compare values of type fn()"
        );
    }

    #[test]
    fn condition_and_branch_types() {
        assert_eq!(error("if 1 { 2 }"), "1:4: expected bool, found int");
        assert_eq!(error("while 1 {}"), "1:7: expected bool, found int");
        assert_eq!(error("if true { 1 }"), "1:11: expected (), found int");
        assert_eq!(
            error("if true { 1 } else { false }"),
            "1:22: expected int, found bool"
        );
        assert_eq!(error("while true { 1 }"), "1:14: expected (), found int");
    }

    #[test]
    fn assignment_keeps_type() {
        assert_eq!(
            error("let x = 1; x = true;"),
            "1:16: expected int, found bool"
        );
    }

    #[test]
    fn return_types() {
        assert_eq!(
            error("fn f() -> int { true }"),
            "1:17: expected int, found bool"
        );
        assert_eq!(error("fn f() -> int {}"), "1:4: expected int, found ()");
        assert_eq!(
            error("fn f() -> int { return; }"),
            "1:17: expected int, found ()"
        );
        assert_eq!(
            error("fn f() { return 1; }"),
            "1:17: expected (), found int"
        );
        assert_eq!(
            error("fn f() -> int { while true { return 1; } }"),
            "1:17: expected int, found ()"
        );
    }

    #[test]
    fn call_errors() {
        assert_eq!(
            error("fn add(a: int, b: int) -> int { a + b } add(1)"),
            "1:41: add expects 2 arguments, got 1"
        );
        assert_eq!(
            error("fn add(a: int, b: int) -> int { a + b } add(1, true)"),
            "1:48: expected int, found bool"
        );
        assert_eq!(
            error("let f = 2; f()"),
            "1:12: expected function, found int"
        );
        assert_eq!(
            error("let print = 1; print(2)"),
            "1:16: expected function, found int"
        );
        assert_eq!(
            error(
                "fn add(a: int, b: int) -> int { a + b } fn h(f: fn(int) -> int) -> int { f(1) } h(add)"
            ),
            "1:83: expected fn(int) -> int, found fn(int, int) -> int"
        );
    }
}
