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
    AssignToImmutable(String),
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
            CheckErrorKind::AssignToImmutable(name) => {
                write!(f, "cannot assign to immutable variable: {name}")
            }
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

#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub kind: WarningKind,
    pub pos: Pos,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WarningKind {
    UnusedVariable(String),
    UnusedMut(String),
}

impl WarningKind {
    fn at(self, pos: Pos) -> Warning {
        Warning { kind: self, pos }
    }
}

impl fmt::Display for WarningKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WarningKind::UnusedVariable(name) => write!(f, "unused variable: {name}"),
            WarningKind::UnusedMut(name) => {
                write!(f, "variable does not need to be mutable: {name}")
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Env {
    locals: Vec<Local>,
    functions: Vec<(String, Type)>,
}

#[derive(Debug, Clone)]
struct Local {
    name: String,
    ty: Type,
    mutable: bool,
    assigned: bool,
    used: bool,
    pos: Pos,
}

impl Local {
    fn new(name: &str, ty: Type, mutable: bool, pos: Pos) -> Self {
        Self {
            name: name.to_string(),
            ty,
            mutable,
            assigned: false,
            used: false,
            pos,
        }
    }

    fn warnings(&self) -> Vec<Warning> {
        let mut warnings = Vec::new();
        if !self.used && !self.name.starts_with('_') {
            warnings.push(WarningKind::UnusedVariable(self.name.clone()).at(self.pos));
        }
        if self.mutable && !self.assigned {
            warnings.push(WarningKind::UnusedMut(self.name.clone()).at(self.pos));
        }
        warnings
    }
}

#[derive(Debug)]
pub struct Checked {
    pub env: Env,
    pub errors: Vec<CheckError>,
    pub warnings: Vec<Warning>,
}

pub fn check(program: &Program, env: &Env) -> Checked {
    let decls: Vec<&FnDecl> = program
        .stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Fn(decl) => Some(decl),
            _ => None,
        })
        .collect();
    let mut functions = env.functions.clone();
    functions.extend(decls.iter().map(|decl| (decl.name.clone(), decl.ty())));

    let mut checker = Checker::new(&functions, env.locals.clone(), Type::Unit);
    for (i, decl) in decls.iter().enumerate() {
        if decls[..i].iter().any(|other| other.name == decl.name) {
            checker.error(
                CheckErrorKind::DuplicateFunction(decl.name.clone()),
                decl.pos,
            );
        }
        checker.function(decl);
    }
    checker.stmts(&program.stmts);
    if let Some(expr) = &program.expr {
        checker.expr(expr);
    }

    let Checker {
        locals,
        mut errors,
        warnings,
        ..
    } = checker;
    errors.sort_by_key(|e| e.pos);
    Checked {
        env: Env { locals, functions },
        errors,
        warnings,
    }
}

// Warnings for top-level variables, once no more code can use them.
pub fn finish(env: &Env) -> Vec<Warning> {
    env.locals.iter().flat_map(Local::warnings).collect()
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

fn fits(found: &Type, expected: &Type) -> bool {
    found == expected || matches!(found, Type::Never | Type::Error) || *expected == Type::Error
}

struct Checker<'a> {
    functions: &'a [(String, Type)],
    locals: Vec<Local>,
    ret: Type,
    diverges: bool,
    errors: Vec<CheckError>,
    warnings: Vec<Warning>,
}

impl<'a> Checker<'a> {
    fn new(functions: &'a [(String, Type)], locals: Vec<Local>, ret: Type) -> Self {
        Self {
            functions,
            locals,
            ret,
            diverges: false,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn error(&mut self, kind: CheckErrorKind, pos: Pos) -> Type {
        self.errors.push(kind.at(pos));
        Type::Error
    }

    fn expect(&mut self, expected: &Type, found: &Type, pos: Pos) {
        if !fits(found, expected) {
            let kind = CheckErrorKind::Mismatch {
                expected: expected.clone(),
                found: found.clone(),
            };
            self.error(kind, pos);
        }
    }

    fn join(&mut self, first: Type, second: Type, pos: Pos) -> Type {
        if matches!(first, Type::Never | Type::Error) {
            return second;
        }
        self.expect(&first, &second, pos);
        first
    }

    fn function(&mut self, decl: &FnDecl) {
        let params = decl
            .params
            .iter()
            .map(|p| Local::new(&p.name, p.ty.clone(), p.mutable, p.pos))
            .collect();
        let mut body = Checker::new(self.functions, params, decl.ret.clone());
        let ty = body.block(&decl.body);
        body.expect(&decl.ret, &ty, block_value_pos(&decl.body, decl.pos));
        body.end_scope(0);
        self.errors.extend(body.errors);
        self.warnings.extend(body.warnings);
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Let {
                    name,
                    mutable,
                    value,
                    pos,
                } => {
                    let ty = self.expr(value);
                    self.locals.push(Local::new(name, ty, *mutable, *pos));
                }
                Stmt::Assign { name, value, pos } => {
                    let found = self.expr(value);
                    let Some(local) = self.local(name) else {
                        self.error(CheckErrorKind::UndefinedVariable(name.clone()), *pos);
                        continue;
                    };
                    local.assigned = true;
                    let (mutable, ty) = (local.mutable, local.ty.clone());
                    if !mutable {
                        self.error(CheckErrorKind::AssignToImmutable(name.clone()), *pos);
                    }
                    self.expect(&ty, &found, value_pos(value));
                }
                Stmt::Expr(expr) => {
                    self.expr(expr);
                }
                Stmt::Fn(_) => {}
            }
        }
    }

    fn block(&mut self, block: &Block) -> Type {
        let outer = self.locals.len();
        self.stmts(&block.stmts);
        let ty = match &block.expr {
            Some(value) => self.expr(value),
            None if self.diverges => Type::Never,
            None => Type::Unit,
        };
        self.end_scope(outer);
        ty
    }

    fn end_scope(&mut self, outer: usize) {
        let ended = self.locals.split_off(outer);
        self.warnings.extend(ended.iter().flat_map(Local::warnings));
    }

    fn local(&mut self, name: &str) -> Option<&mut Local> {
        self.locals.iter_mut().rfind(|local| local.name == name)
    }

    fn lookup(&mut self, name: &str) -> Option<Type> {
        if let Some(local) = self.local(name) {
            local.used = true;
            return Some(local.ty.clone());
        }
        let (_, ty) = self.functions.iter().rfind(|(n, _)| n == name)?;
        Some(ty.clone())
    }

    fn expr(&mut self, expr: &Expr) -> Type {
        let pos = expr.pos;
        match &expr.kind {
            ExprKind::Int(_) => Type::Int,
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Str(_) => Type::Str,
            ExprKind::Var(name) => self.lookup(name).unwrap_or_else(|| {
                self.error(CheckErrorKind::UndefinedVariable(name.clone()), pos)
            }),
            ExprKind::Call { name, args } => {
                let callee = match self.lookup(name) {
                    Some(ty) => ty,
                    None if name == "print" => return self.print(args, pos),
                    None => self.error(CheckErrorKind::UndefinedFunction(name.clone()), pos),
                };
                self.call(name, callee, args, pos)
            }
            ExprKind::Unary { op, operand } => {
                let ty = match op {
                    UnaryOp::Neg => Type::Int,
                    UnaryOp::Not => Type::Bool,
                };
                let found = self.expr(operand);
                self.expect(&ty, &found, value_pos(operand));
                ty
            }
            ExprKind::Binary { left, rest } => {
                let mut ty = self.expr(left);
                let mut ty_pos = value_pos(left);
                for (op, pos, right) in rest {
                    ty = self.binary(op, ty, ty_pos, right, *pos);
                    ty_pos = *pos;
                }
                ty
            }
            ExprKind::Logical { left, rest } => {
                let found = self.expr(left);
                self.expect(&Type::Bool, &found, value_pos(left));
                let diverges = self.diverges;
                for (_, _, right) in rest {
                    let found = self.expr(right);
                    self.expect(&Type::Bool, &found, value_pos(right));
                }
                self.diverges = diverges;
                Type::Bool
            }
            ExprKind::Block(block) => self.block(block),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let found = self.expr(cond);
                self.expect(&Type::Bool, &found, value_pos(cond));
                let diverges = self.diverges;
                let then_ty = self.block(then_branch);
                match else_branch {
                    Some(else_branch) => {
                        let then_diverges = self.diverges;
                        self.diverges = diverges;
                        let else_ty = self.expr(else_branch);
                        self.diverges = then_diverges && self.diverges;
                        self.join(then_ty, else_ty, value_pos(else_branch))
                    }
                    None => {
                        self.expect(&Type::Unit, &then_ty, block_value_pos(then_branch, pos));
                        self.diverges = diverges;
                        Type::Unit
                    }
                }
            }
            ExprKind::While { cond, body } => {
                let found = self.expr(cond);
                self.expect(&Type::Bool, &found, value_pos(cond));
                let diverges = self.diverges;
                let body_ty = self.block(body);
                self.expect(&Type::Unit, &body_ty, block_value_pos(body, pos));
                self.diverges = diverges;
                Type::Unit
            }
            ExprKind::Return(value) => {
                let (found, pos) = match value {
                    Some(value) => (self.expr(value), value_pos(value)),
                    None => (Type::Unit, pos),
                };
                let ret = self.ret.clone();
                self.expect(&ret, &found, pos);
                self.diverges = true;
                Type::Never
            }
            ExprKind::Break | ExprKind::Continue => {
                self.diverges = true;
                Type::Never
            }
        }
    }

    fn call(&mut self, name: &str, callee: Type, args: &[Expr], pos: Pos) -> Type {
        let Type::Fn { params, ret } = callee else {
            if callee != Type::Error {
                self.error(CheckErrorKind::NotAFunction(callee), pos);
            }
            self.args(args);
            return Type::Error;
        };
        if args.len() != params.len() {
            let kind = CheckErrorKind::ArgumentCount {
                name: name.to_string(),
                expected: params.len(),
                found: args.len(),
            };
            self.error(kind, pos);
            self.args(args);
            return *ret;
        }
        for (arg, param) in args.iter().zip(&params) {
            let found = self.expr(arg);
            self.expect(param, &found, value_pos(arg));
        }
        *ret
    }

    fn args(&mut self, args: &[Expr]) {
        for arg in args {
            self.expr(arg);
        }
    }

    fn binary(
        &mut self,
        op: &BinaryOp,
        left_ty: Type,
        left_pos: Pos,
        right: &Expr,
        pos: Pos,
    ) -> Type {
        let right_ty = self.expr(right);
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            let ty = self.join(left_ty, right_ty, value_pos(right));
            if matches!(ty, Type::Fn { .. }) {
                self.error(CheckErrorKind::CannotCompare(ty), pos);
            }
            return Type::Bool;
        }
        let strings = matches!(
            op,
            BinaryOp::Add | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) && (left_ty == Type::Str || right_ty == Type::Str);
        let operand = if strings { Type::Str } else { Type::Int };
        self.expect(&operand, &left_ty, left_pos);
        self.expect(&operand, &right_ty, value_pos(right));
        match op {
            BinaryOp::Add => operand,
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => Type::Int,
            _ => Type::Bool,
        }
    }

    fn print(&mut self, args: &[Expr], pos: Pos) -> Type {
        if args.len() != 1 {
            self.error(CheckErrorKind::PrintArgumentCount(args.len()), pos);
        }
        self.args(args);
        Type::Unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn parse(input: &str) -> Program {
        Parser::new(tokenize(input).unwrap()).parse().unwrap()
    }

    fn check_program(input: &str) -> Checked {
        let mut checked = check(&parse(input), &Env::default());
        checked.warnings.extend(finish(&checked.env));
        checked
    }

    fn warnings(input: &str) -> Vec<String> {
        let checked = check_program(input);
        assert_eq!(checked.errors, [], "{input}");
        checked.warnings.iter().map(|w| w.to_string()).collect()
    }

    fn errors(input: &str) -> Vec<String> {
        let checked = check_program(input);
        checked.errors.iter().map(|e| e.to_string()).collect()
    }

    fn error(input: &str) -> String {
        let [error] = errors(input).try_into().unwrap_or_else(|errors| {
            panic!("expected one error in {input}, got {errors:?}");
        });
        error
    }

    #[test]
    fn well_typed_programs() {
        for input in [
            "(1 + 2 * 3 < 7) == true",
            "let mut x = 1; x = x + 1; x",
            "if true { 1 } else { 2 }",
            "if true { print(1); }",
            "print(1) == print(2)",
            "let mut i = 0; while i < 3 { i = i + 1; if i == 2 { break; } }",
            "fn f() {} let g = f; g()",
            "fn print(x: bool) -> bool { x } print(true) && true",
            "fn twice(f: fn(int) -> int, x: int) -> int { f(f(x)) }
             fn inc(x: int) -> int { x + 1 }
             twice(inc, 1)",
            "fn even(n: int) -> bool { if n == 0 { true } else { odd(n - 1) } }
             fn odd(n: int) -> bool { if n == 0 { false } else { even(n - 1) } }",
            "let mut x = 1; { let x = true; } x = 2;",
            "fn f(n: int) -> int { let mut n = n; n = n + 1; n }",
            r#"fn greet(name: str) -> str { "hi, " + name } greet("Rock") == "hi, Rock""#,
            r#"let mut s = ""; s = s + "a"; print(s); s != "b""#,
            r#""a" < "b" && "b" >= "a""#,
        ] {
            assert_eq!(check_program(input).errors, [], "{input}");
        }
    }

    #[test]
    fn used_variables_are_not_warned() {
        for input in [
            "let mut x = 1; x = 2; x",
            "let mut i = 0; while i < 3 { i = i + 1; }",
            "fn f(mut n: int) -> int { n = n + 1; n }",
            "let x = 1; { let mut y = x; y = 2; y }",
            "fn f(g: fn()) { g() }",
        ] {
            assert_eq!(warnings(input), [] as [String; 0], "{input}");
        }
    }

    #[test]
    fn unused_mut() {
        assert_eq!(
            warnings("let mut x = 1; x"),
            ["1:9: variable does not need to be mutable: x"]
        );
        assert_eq!(
            warnings("fn f(mut n: int) -> int { n }"),
            ["1:10: variable does not need to be mutable: n"]
        );
        assert_eq!(
            warnings("let mut x = 1; let mut x = x; x = 3; x"),
            ["1:9: variable does not need to be mutable: x"]
        );
        assert_eq!(
            warnings("let mut x = 1; { let mut y = x; y }"),
            [
                "1:26: variable does not need to be mutable: y",
                "1:9: variable does not need to be mutable: x"
            ]
        );
    }

    #[test]
    fn unused_variables() {
        assert_eq!(warnings("let x = 1;"), ["1:5: unused variable: x"]);
        assert_eq!(warnings("fn f(n: int) {}"), ["1:6: unused variable: n"]);
        assert_eq!(
            warnings("let mut x = 1; x = 2;"),
            ["1:9: unused variable: x"]
        );
        assert_eq!(
            warnings("let x = 1; let x = 2; x"),
            ["1:5: unused variable: x"]
        );
        assert_eq!(
            warnings("let mut x = 1;"),
            [
                "1:9: unused variable: x",
                "1:9: variable does not need to be mutable: x"
            ]
        );
    }

    #[test]
    fn underscore_allows_unused_variables() {
        for input in ["let _x = 1;", "let _ = 1;", "fn f(_n: int) {}"] {
            assert_eq!(warnings(input), [] as [String; 0], "{input}");
        }
        assert_eq!(
            warnings("let mut _x = 1;"),
            ["1:9: variable does not need to be mutable: _x"]
        );
    }

    #[test]
    fn top_level_warnings_wait_for_finish() {
        let checked = check(&parse("let mut x = 1; x"), &Env::default());
        assert_eq!(checked.warnings, []);
        assert_eq!(finish(&checked.env).len(), 1);
        let checked = check(&parse("x = 2;"), &checked.env);
        assert_eq!(checked.errors, []);
        assert_eq!(finish(&checked.env), []);
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
            assert_eq!(check_program(input).errors, [], "{input}");
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
        assert_eq!(error("false < 1"), "1:1: expected int, found bool");
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
        assert_eq!(error("if 1 {}"), "1:4: expected bool, found int");
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
            error("let mut x = 1; x = true;"),
            "1:20: expected int, found bool"
        );
    }

    #[test]
    fn assignment_needs_mut() {
        assert_eq!(
            error("let x = 1; x = 2;"),
            "1:12: cannot assign to immutable variable: x"
        );
        assert_eq!(
            error("fn f(n: int) { n = 1; }"),
            "1:16: cannot assign to immutable variable: n"
        );
        assert_eq!(
            error("fn f(mut n: int) { let n = 1; n = 2; }"),
            "1:31: cannot assign to immutable variable: n"
        );
        assert_eq!(
            error("let mut x = 1; { let x = 2; x = 3; }"),
            "1:29: cannot assign to immutable variable: x"
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

    #[test]
    fn several_errors() {
        assert_eq!(
            errors("false < true"),
            [
                "1:1: expected int, found bool",
                "1:9: expected int, found bool"
            ]
        );
        assert_eq!(
            errors("let a = b;\nfn f() -> int { true }\na = 1;"),
            [
                "1:9: undefined variable: b",
                "2:17: expected int, found bool",
                "3:1: cannot assign to immutable variable: a"
            ]
        );
        assert_eq!(
            errors("fn f(a: int) -> int { a } f(1, y)"),
            [
                "1:27: f expects 1 argument, got 2",
                "1:32: undefined variable: y"
            ]
        );
        assert_eq!(
            errors("print(y, 1)"),
            [
                "1:1: print expects 1 argument, got 2",
                "1:7: undefined variable: y"
            ]
        );
    }

    #[test]
    fn errors_do_not_cascade() {
        for (input, expected) in [
            ("let x = y; x + 1", "1:9: undefined variable: y"),
            ("let x = y; x()", "1:9: undefined variable: y"),
            ("f(1) + 1", "1:1: undefined function: f"),
            ("let f = g; f(1, 2) == true", "1:9: undefined variable: g"),
            ("let mut x = y; x = true;", "1:13: undefined variable: y"),
            (
                "if true { y } else { 1 } + 1",
                "1:11: undefined variable: y",
            ),
        ] {
            assert_eq!(error(input), expected);
        }
    }

    #[test]
    fn warnings_come_with_errors() {
        let checked = check_program("let x = 1; y");
        let warnings: Vec<String> = checked.warnings.iter().map(|w| w.to_string()).collect();
        assert_eq!(warnings, ["1:5: unused variable: x"]);
        assert_eq!(checked.errors.len(), 1);
    }

    #[test]
    fn string_operand_types() {
        assert_eq!(error(r#""a" + 1"#), "1:7: expected str, found int");
        assert_eq!(error(r#"1 + "a""#), "1:1: expected str, found int");
        assert_eq!(error(r#""a" == 1"#), "1:8: expected str, found int");
        assert_eq!(error(r#"-"a""#), "1:2: expected int, found str");
        assert_eq!(error(r#""a" < 1"#), "1:7: expected str, found int");
        assert_eq!(
            errors(r#""a" - "b""#),
            [
                "1:1: expected int, found str",
                "1:7: expected int, found str"
            ]
        );
    }
}
