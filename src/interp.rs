use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};

use crate::ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, LogicalOp, Program, Stmt, UnaryOp};
use crate::lexer::Pos;
use crate::value::Value;

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
    UndefinedVariable(String),
    UndefinedFunction(String),
    ArgumentCount {
        name: String,
        expected: usize,
        found: usize,
    },
    TypeMismatch {
        expected: &'static str,
        found: Value,
    },
    CannotCompare(Value, Value),
    DivisionByZero,
    IntegerOverflow,
    StackOverflow,
    Output(io::ErrorKind),
}

impl RuntimeErrorKind {
    fn at(self, pos: Pos) -> RuntimeError {
        RuntimeError { kind: self, pos }
    }
}

impl fmt::Display for RuntimeErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RuntimeErrorKind::UndefinedVariable(name) => write!(f, "undefined variable: {name}"),
            RuntimeErrorKind::UndefinedFunction(name) => write!(f, "undefined function: {name}"),
            RuntimeErrorKind::ArgumentCount {
                name,
                expected,
                found,
            } => {
                let s = if *expected == 1 { "" } else { "s" };
                write!(f, "{name} expects {expected} argument{s}, got {found}")
            }
            RuntimeErrorKind::TypeMismatch { expected, found } => {
                write!(f, "expected {expected}, got {found}")
            }
            RuntimeErrorKind::CannotCompare(l, r) => write!(f, "cannot compare {l} with {r}"),
            RuntimeErrorKind::DivisionByZero => write!(f, "division by zero"),
            RuntimeErrorKind::IntegerOverflow => write!(f, "integer overflow"),
            RuntimeErrorKind::StackOverflow => {
                write!(f, "stack overflow: more than {MAX_CALL_DEPTH} nested calls")
            }
            RuntimeErrorKind::Output(kind) => write!(f, "cannot write output: {kind}"),
        }
    }
}

enum Unwind {
    Return(Value),
    Error(RuntimeError),
}

impl From<RuntimeError> for Unwind {
    fn from(error: RuntimeError) -> Self {
        Unwind::Error(error)
    }
}

pub const MAX_CALL_DEPTH: usize = 1000;
// Each nested call takes several Rust frames; in debug builds that can be ~40 KiB per call.
pub const STACK_SIZE: usize = 256 * 1024 * 1024;

pub struct Interpreter<W: Write> {
    scopes: Vec<HashMap<String, Value>>,
    depth: usize,
    out: W,
}

impl<W: Write> Interpreter<W> {
    pub fn new(out: W) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            depth: 0,
            out,
        }
    }

    pub fn run(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        self.sequence(&program.stmts, program.expr.as_ref())
            .map_err(|unwind| match unwind {
                Unwind::Error(error) => error,
                Unwind::Return(_) => unreachable!("the parser rejects 'return' outside functions"),
            })
    }

    fn block(&mut self, block: &Block) -> Result<Value, Unwind> {
        self.scopes.push(HashMap::new());
        let result = self.sequence(&block.stmts, block.expr.as_deref());
        self.scopes.pop();
        result
    }

    fn sequence(&mut self, stmts: &[Stmt], expr: Option<&Expr>) -> Result<Value, Unwind> {
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, value } => {
                    let value = self.eval(value)?;
                    self.define(name, value);
                }
                Stmt::Assign { name, value, pos } => {
                    let value = self.eval(value)?;
                    self.assign(name, value).map_err(|kind| kind.at(*pos))?;
                }
                Stmt::Fn(decl) => self.define(&decl.name, Value::Function(decl.clone())),
                Stmt::Expr(expr) => {
                    self.eval(expr)?;
                }
            }
        }
        match expr {
            Some(expr) => self.eval(expr),
            None => Ok(Value::Unit),
        }
    }

    fn define(&mut self, name: &str, value: Value) {
        self.scopes
            .last_mut()
            .expect("global scope is never popped")
            .insert(name.to_string(), value);
    }

    fn assign(&mut self, name: &str, value: Value) -> Result<(), RuntimeErrorKind> {
        let slot = self
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(name))
            .ok_or_else(|| RuntimeErrorKind::UndefinedVariable(name.to_string()))?;
        *slot = value;
        Ok(())
    }

    fn lookup(&self, name: &str) -> Result<Value, RuntimeErrorKind> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .ok_or_else(|| RuntimeErrorKind::UndefinedVariable(name.to_string()))
    }

    fn eval(&mut self, expr: &Expr) -> Result<Value, Unwind> {
        let at = |kind: RuntimeErrorKind| kind.at(expr.pos);
        match &expr.kind {
            ExprKind::Int(n) => Ok(Value::Int(*n)),
            ExprKind::Bool(b) => Ok(Value::Bool(*b)),
            ExprKind::Var(name) => Ok(self.lookup(name).map_err(at)?),
            ExprKind::Call { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.eval(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call(name, args, expr.pos)
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                Ok(binary(op, l, r).map_err(at)?)
            }
            ExprKind::Unary { op, operand } => {
                let value = self.eval(operand)?;
                Ok(unary(op, value).map_err(at)?)
            }
            ExprKind::Logical { op, left, right } => {
                let l = boolean(self.eval(left)?).map_err(at)?;
                match (op, l) {
                    (LogicalOp::And, false) => Ok(Value::Bool(false)),
                    (LogicalOp::Or, true) => Ok(Value::Bool(true)),
                    _ => Ok(Value::Bool(boolean(self.eval(right)?).map_err(at)?)),
                }
            }
            ExprKind::Block(block) => self.block(block),
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                if self.condition(cond)? {
                    self.block(then_branch)
                } else if let Some(else_branch) = else_branch {
                    self.eval(else_branch)
                } else {
                    Ok(Value::Unit)
                }
            }
            ExprKind::While { cond, body } => {
                while self.condition(cond)? {
                    self.block(body)?;
                }
                Ok(Value::Unit)
            }
            ExprKind::Return(value) => {
                let value = match value {
                    Some(value) => self.eval(value)?,
                    None => Value::Unit,
                };
                Err(Unwind::Return(value))
            }
        }
    }

    fn condition(&mut self, cond: &Expr) -> Result<bool, Unwind> {
        let value = self.eval(cond)?;
        Ok(boolean(value).map_err(|kind| kind.at(cond.pos))?)
    }

    fn call(&mut self, name: &str, args: Vec<Value>, pos: Pos) -> Result<Value, Unwind> {
        match self.lookup(name) {
            Ok(Value::Function(decl)) => self.call_function(&decl, args, pos),
            Ok(found) => Err(RuntimeErrorKind::TypeMismatch {
                expected: "function",
                found,
            }
            .at(pos)
            .into()),
            Err(_) if name == "print" => {
                let [value] = args.as_slice() else {
                    return Err(RuntimeErrorKind::ArgumentCount {
                        name: name.to_string(),
                        expected: 1,
                        found: args.len(),
                    }
                    .at(pos)
                    .into());
                };
                writeln!(self.out, "{value}")
                    .map_err(|e| RuntimeErrorKind::Output(e.kind()).at(pos))?;
                Ok(Value::Unit)
            }
            _ => Err(RuntimeErrorKind::UndefinedFunction(name.to_string())
                .at(pos)
                .into()),
        }
    }

    fn call_function(
        &mut self,
        decl: &FnDecl,
        args: Vec<Value>,
        pos: Pos,
    ) -> Result<Value, Unwind> {
        if args.len() != decl.params.len() {
            return Err(RuntimeErrorKind::ArgumentCount {
                name: decl.name.clone(),
                expected: decl.params.len(),
                found: args.len(),
            }
            .at(pos)
            .into());
        }
        if self.depth == MAX_CALL_DEPTH {
            return Err(RuntimeErrorKind::StackOverflow.at(pos).into());
        }
        let params = decl.params.iter().cloned().zip(args).collect();
        let caller_scopes = self.scopes.split_off(1);
        self.scopes.push(params);
        self.depth += 1;
        let result = self.sequence(&decl.body.stmts, decl.body.expr.as_deref());
        self.depth -= 1;
        self.scopes.truncate(1);
        self.scopes.extend(caller_scopes);
        match result {
            Err(Unwind::Return(value)) => Ok(value),
            other => other,
        }
    }
}

fn unary(op: &UnaryOp, value: Value) -> Result<Value, RuntimeErrorKind> {
    match op {
        UnaryOp::Neg => checked(int(value)?.checked_neg()),
        UnaryOp::Not => Ok(Value::Bool(!boolean(value)?)),
    }
}

fn binary(op: &BinaryOp, l: Value, r: Value) -> Result<Value, RuntimeErrorKind> {
    match op {
        BinaryOp::Eq => Ok(Value::Bool(equal(l, r)?)),
        BinaryOp::Ne => Ok(Value::Bool(!equal(l, r)?)),
        BinaryOp::Lt => Ok(Value::Bool(int(l)? < int(r)?)),
        BinaryOp::Le => Ok(Value::Bool(int(l)? <= int(r)?)),
        BinaryOp::Gt => Ok(Value::Bool(int(l)? > int(r)?)),
        BinaryOp::Ge => Ok(Value::Bool(int(l)? >= int(r)?)),
        BinaryOp::Add => checked(int(l)?.checked_add(int(r)?)),
        BinaryOp::Sub => checked(int(l)?.checked_sub(int(r)?)),
        BinaryOp::Mul => checked(int(l)?.checked_mul(int(r)?)),
        BinaryOp::Div => {
            let (l, r) = (int(l)?, int(r)?);
            if r == 0 {
                return Err(RuntimeErrorKind::DivisionByZero);
            }
            checked(l.checked_div(r))
        }
    }
}

fn equal(l: Value, r: Value) -> Result<bool, RuntimeErrorKind> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::Unit, Value::Unit) => Ok(true),
        (l, r) => Err(RuntimeErrorKind::CannotCompare(l, r)),
    }
}

fn int(value: Value) -> Result<i64, RuntimeErrorKind> {
    match value {
        Value::Int(n) => Ok(n),
        found => Err(RuntimeErrorKind::TypeMismatch {
            expected: "integer",
            found,
        }),
    }
}

fn boolean(value: Value) -> Result<bool, RuntimeErrorKind> {
    match value {
        Value::Bool(b) => Ok(b),
        found => Err(RuntimeErrorKind::TypeMismatch {
            expected: "bool",
            found,
        }),
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
    use crate::lexer::tokenize;
    use crate::parser::Parser;
    use std::thread;

    fn exec(interp: &mut Interpreter<Vec<u8>>, input: &str) -> Result<Value, RuntimeErrorKind> {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        interp.run(&program).map_err(|error| error.kind)
    }

    fn error_at(input: &str) -> String {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        let error = Interpreter::new(Vec::new()).run(&program).unwrap_err();
        error.to_string()
    }

    fn value(input: &str) -> Result<Value, RuntimeErrorKind> {
        exec(&mut Interpreter::new(Vec::new()), input)
    }

    fn run(input: &str) -> Result<i64, RuntimeErrorKind> {
        match exec(&mut Interpreter::new(Vec::new()), input)? {
            Value::Int(n) => Ok(n),
            other => panic!("expected integer, got {other}"),
        }
    }

    fn output(input: &str) -> Result<String, RuntimeErrorKind> {
        let mut interp = Interpreter::new(Vec::new());
        exec(&mut interp, input)?;
        Ok(String::from_utf8(interp.out).unwrap())
    }

    fn mismatch(expected: &'static str, found: Value) -> RuntimeErrorKind {
        RuntimeErrorKind::TypeMismatch { expected, found }
    }

    fn with_big_stack(f: impl FnOnce() + Send + 'static) {
        thread::Builder::new()
            .stack_size(STACK_SIZE)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    fn undefined(name: &str) -> RuntimeErrorKind {
        RuntimeErrorKind::UndefinedVariable(name.to_string())
    }

    #[test]
    fn integer() {
        assert_eq!(run("42"), Ok(42));
    }

    #[test]
    fn addition() {
        assert_eq!(run("1 + 2"), Ok(3));
    }

    #[test]
    fn subtraction() {
        assert_eq!(run("5 - 3"), Ok(2));
    }

    #[test]
    fn multiplication() {
        assert_eq!(run("4 * 3"), Ok(12));
    }

    #[test]
    fn division_truncates() {
        assert_eq!(run("7 / 2"), Ok(3));
    }

    #[test]
    fn precedence() {
        assert_eq!(run("1 + 2 * 3"), Ok(7));
    }

    #[test]
    fn parentheses() {
        assert_eq!(run("(1 + 2) * 3"), Ok(9));
    }

    #[test]
    fn left_associative() {
        assert_eq!(run("10 - 3 - 2"), Ok(5));
    }

    #[test]
    fn negation() {
        assert_eq!(run("-5"), Ok(-5));
    }

    #[test]
    fn double_negation() {
        assert_eq!(run("--5"), Ok(5));
    }

    #[test]
    fn negation_of_expression() {
        assert_eq!(run("-(1 + 2) * 3"), Ok(-9));
    }

    #[test]
    fn computed_min_value() {
        assert_eq!(run("-9223372036854775807 - 1"), Ok(i64::MIN));
    }

    #[test]
    fn min_value_literal() {
        assert_eq!(run("-9223372036854775808"), Ok(i64::MIN));
    }

    #[test]
    fn bool_literals() {
        assert_eq!(value("true"), Ok(Value::Bool(true)));
        assert_eq!(value("false"), Ok(Value::Bool(false)));
    }

    #[test]
    fn bool_variable() {
        assert_eq!(value("let b = true; b"), Ok(Value::Bool(true)));
    }

    #[test]
    fn comparisons() {
        assert_eq!(value("1 < 2"), Ok(Value::Bool(true)));
        assert_eq!(value("2 <= 2"), Ok(Value::Bool(true)));
        assert_eq!(value("1 > 2"), Ok(Value::Bool(false)));
        assert_eq!(value("1 >= 2"), Ok(Value::Bool(false)));
    }

    #[test]
    fn equality() {
        assert_eq!(value("1 + 1 == 2"), Ok(Value::Bool(true)));
        assert_eq!(value("1 != 2"), Ok(Value::Bool(true)));
        assert_eq!(value("true == false"), Ok(Value::Bool(false)));
        assert_eq!(value("(1 < 2) != false"), Ok(Value::Bool(true)));
        assert_eq!(value("print(1) == print(2)"), Ok(Value::Bool(true)));
    }

    #[test]
    fn logical_not() {
        assert_eq!(value("!true"), Ok(Value::Bool(false)));
        assert_eq!(value("!(1 > 2)"), Ok(Value::Bool(true)));
    }

    #[test]
    fn and() {
        assert_eq!(value("true && true"), Ok(Value::Bool(true)));
        assert_eq!(value("true && false"), Ok(Value::Bool(false)));
        assert_eq!(value("false && true"), Ok(Value::Bool(false)));
        assert_eq!(value("false && false"), Ok(Value::Bool(false)));
    }

    #[test]
    fn or() {
        assert_eq!(value("true || true"), Ok(Value::Bool(true)));
        assert_eq!(value("true || false"), Ok(Value::Bool(true)));
        assert_eq!(value("false || true"), Ok(Value::Bool(true)));
        assert_eq!(value("false || false"), Ok(Value::Bool(false)));
    }

    #[test]
    fn short_circuit() {
        assert_eq!(
            output("false && print(1); true || print(2);"),
            Ok(String::new())
        );
        assert_eq!(value("false && 1 / 0 == 0"), Ok(Value::Bool(false)));
    }

    #[test]
    fn block_value() {
        assert_eq!(run("{ let x = 2; x * 3 }"), Ok(6));
    }

    #[test]
    fn empty_block_is_unit() {
        assert_eq!(value("{}"), Ok(Value::Unit));
        assert_eq!(value("{ 1; }"), Ok(Value::Unit));
    }

    #[test]
    fn block_sees_outer_variables() {
        assert_eq!(run("let x = 1; { x + 1 }"), Ok(2));
    }

    #[test]
    fn shadowing_in_block_ends_with_block() {
        assert_eq!(run("let x = 1; { let x = 2; } x"), Ok(1));
        assert_eq!(run("let x = 1; let y = { let x = 2; x }; x + y"), Ok(3));
    }

    #[test]
    fn if_else() {
        assert_eq!(run("if 1 < 2 { 10 } else { 20 }"), Ok(10));
        assert_eq!(run("if 1 > 2 { 10 } else { 20 }"), Ok(20));
    }

    #[test]
    fn if_without_else_is_unit() {
        assert_eq!(value("if false { 1 }"), Ok(Value::Unit));
    }

    #[test]
    fn else_if_chain() {
        let program = "let x = 5; if x < 0 { 1 } else if x < 10 { 2 } else { 3 }";
        assert_eq!(run(program), Ok(2));
    }

    #[test]
    fn if_value_in_let() {
        assert_eq!(run("let x = if true { 1 } else { 2 }; x + 1"), Ok(2));
    }

    #[test]
    fn untaken_branch_is_not_evaluated() {
        assert_eq!(
            output("if true { print(1); } else { print(2); }"),
            Ok("1\n".to_string())
        );
        assert_eq!(run("if false { 1 / 0 } else { 2 }"), Ok(2));
    }

    #[test]
    fn if_branch_has_own_scope() {
        assert_eq!(run("let x = 1; if true { let x = 2; } x"), Ok(1));
    }

    #[test]
    fn assignment() {
        assert_eq!(run("let x = 1; x = x + 1; x"), Ok(2));
    }

    #[test]
    fn assignment_in_block_changes_outer_variable() {
        assert_eq!(run("let x = 1; { x = 2; } x"), Ok(2));
    }

    #[test]
    fn assignment_changes_innermost_variable() {
        assert_eq!(run("let x = 1; { let x = 2; x = 3; } x"), Ok(1));
    }

    #[test]
    fn while_loop() {
        let program = "let i = 0; let sum = 0; while i < 10 { i = i + 1; sum = sum + i; } sum";
        assert_eq!(run(program), Ok(55));
    }

    #[test]
    fn while_false_does_not_run_body() {
        assert_eq!(output("while false { print(1); }"), Ok(String::new()));
    }

    #[test]
    fn while_is_unit() {
        assert_eq!(value("while false {}"), Ok(Value::Unit));
    }

    #[test]
    fn while_body_scope_is_fresh_each_iteration() {
        let program = "let i = 0; while i < 3 { let x = i; i = i + 1; print(x); }";
        assert_eq!(output(program), Ok("0\n1\n2\n".to_string()));
    }

    #[test]
    fn function_call() {
        assert_eq!(run("fn add(a, b) { a + b } add(1, 2)"), Ok(3));
    }

    #[test]
    fn function_declaration_is_unit() {
        assert_eq!(value("fn f() {}"), Ok(Value::Unit));
    }

    #[test]
    fn function_without_parameters() {
        assert_eq!(
            output("fn hello() { print(1); } hello(); hello();"),
            Ok("1\n1\n".to_string())
        );
    }

    #[test]
    fn recursion() {
        let program = "fn fact(n) { if n <= 1 { 1 } else { n * fact(n - 1) } } fact(10)";
        assert_eq!(run(program), Ok(3628800));
    }

    #[test]
    fn function_sees_globals() {
        assert_eq!(run("let base = 10; fn f(x) { base + x } f(1)"), Ok(11));
    }

    #[test]
    fn function_can_assign_global() {
        assert_eq!(
            run("let count = 0; fn inc() { count = count + 1; } inc(); inc(); count"),
            Ok(2)
        );
    }

    #[test]
    fn parameter_shadows_global() {
        assert_eq!(run("let x = 1; fn f(x) { x } f(2) + x"), Ok(3));
    }

    #[test]
    fn locals_do_not_leak_from_function() {
        assert_eq!(run("let x = 1; fn f() { let x = 2; x } f() + x"), Ok(3));
    }

    #[test]
    fn function_as_value() {
        assert_eq!(
            run("fn add(a, b) { a + b } let plus = add; plus(1, 2)"),
            Ok(3)
        );
    }

    #[test]
    fn function_display() {
        assert_eq!(
            output("fn add(a, b) { a + b } print(add);"),
            Ok("<fn add>\n".to_string())
        );
    }

    #[test]
    fn let_shadows_function() {
        assert_eq!(run("fn f() { 1 } let f = 2; f"), Ok(2));
    }

    #[test]
    fn user_function_shadows_print() {
        assert_eq!(output("fn print(x) { x } print(1);"), Ok(String::new()));
    }

    #[test]
    fn caller_scopes_restored_after_call() {
        assert_eq!(run("fn f() { 1 } { let y = 2; f() + y }"), Ok(3));
    }

    #[test]
    fn early_return() {
        assert_eq!(
            run("fn abs(x) { if x < 0 { return -x; } x } abs(-5) + abs(3)"),
            Ok(8)
        );
    }

    #[test]
    fn return_without_semicolon() {
        assert_eq!(
            run("fn sign(x) { if x < 0 { return -1 } 1 } sign(-3)"),
            Ok(-1)
        );
    }

    #[test]
    fn return_from_loop() {
        let program = "fn root(n) { let i = 0; while true { if i * i >= n { return i; } i = i + 1; } } root(50)";
        assert_eq!(run(program), Ok(8));
    }

    #[test]
    fn return_skips_rest_of_body() {
        assert_eq!(
            output("fn f() { print(1); return; print(2); } f();"),
            Ok("1\n".to_string())
        );
    }

    #[test]
    fn bare_return_is_unit() {
        assert_eq!(value("fn f() { return; 1 } f()"), Ok(Value::Unit));
    }

    #[test]
    fn return_only_leaves_innermost_function() {
        assert_eq!(run("fn g() { return 1; } fn f() { g() + 1 } f()"), Ok(2));
    }

    #[test]
    fn scopes_restored_after_return() {
        assert_eq!(
            run("fn f() { { { return 1; } } } { let y = 2; f() + y }"),
            Ok(3)
        );
    }

    #[test]
    fn deep_recursion_within_limit() {
        with_big_stack(|| {
            let program = "fn down(n) { if n == 1 { return 1; } down(n - 1) } down(1000)";
            assert_eq!(run(program), Ok(1));
        });
    }

    #[test]
    fn let_bindings() {
        assert_eq!(run("let x = 1 + 2; let y = x * 3; y - 1"), Ok(8));
    }

    #[test]
    fn shadowing() {
        assert_eq!(run("let x = 1; let x = x + 1; x"), Ok(2));
    }

    #[test]
    fn statements_only() {
        assert_eq!(
            exec(&mut Interpreter::new(Vec::new()), "let x = 1;"),
            Ok(Value::Unit)
        );
    }

    #[test]
    fn variables_persist_between_runs() {
        let mut interp = Interpreter::new(Vec::new());
        assert_eq!(exec(&mut interp, "let x = 2;"), Ok(Value::Unit));
        assert_eq!(exec(&mut interp, "x * 3"), Ok(Value::Int(6)));
    }

    #[test]
    fn expression_statement_value_is_discarded() {
        assert_eq!(
            exec(&mut Interpreter::new(Vec::new()), "1 + 2;"),
            Ok(Value::Unit)
        );
    }

    #[test]
    fn print() {
        assert_eq!(
            output("let x = 1 + 2 * 3; print(x);"),
            Ok("7\n".to_string())
        );
    }

    #[test]
    fn print_bool() {
        assert_eq!(output("print(false);"), Ok("false\n".to_string()));
    }

    #[test]
    fn print_returns_unit() {
        assert_eq!(output("print(print(1));"), Ok("1\n()\n".to_string()));
    }

    #[test]
    fn output_before_error_is_kept() {
        let mut interp = Interpreter::new(Vec::new());
        assert_eq!(
            exec(&mut interp, "print(1); print(1 / 0);"),
            Err(RuntimeErrorKind::DivisionByZero)
        );
        assert_eq!(String::from_utf8(interp.out).unwrap(), "1\n");
    }

    #[test]
    fn undefined_variable() {
        assert_eq!(run("x + 1"), Err(undefined("x")));
    }

    #[test]
    fn use_before_definition() {
        assert_eq!(run("let y = x; let x = 1; y"), Err(undefined("x")));
    }

    #[test]
    fn block_variable_not_visible_outside() {
        assert_eq!(run("{ let y = 1; } y"), Err(undefined("y")));
    }

    #[test]
    fn scope_is_popped_after_error() {
        let mut interp = Interpreter::new(Vec::new());
        assert_eq!(exec(&mut interp, "let x = 1;"), Ok(Value::Unit));
        assert_eq!(
            exec(&mut interp, "{ let x = 2; 1 / 0 }"),
            Err(RuntimeErrorKind::DivisionByZero)
        );
        assert_eq!(exec(&mut interp, "x"), Ok(Value::Int(1)));
    }

    #[test]
    fn if_condition_must_be_bool() {
        assert_eq!(
            run("if 1 { 2 } else { 3 }"),
            Err(mismatch("bool", Value::Int(1)))
        );
    }

    #[test]
    fn assignment_to_undefined_variable() {
        assert_eq!(run("x = 1; 0"), Err(undefined("x")));
    }

    #[test]
    fn while_condition_must_be_bool() {
        assert_eq!(value("while 1 {}"), Err(mismatch("bool", Value::Int(1))));
    }

    #[test]
    fn error_stops_loop() {
        let mut interp = Interpreter::new(Vec::new());
        let program = "let i = 0; while true { i = i + 1; if i == 3 { 1 / 0 } else { 0 }; }";
        assert_eq!(
            exec(&mut interp, program),
            Err(RuntimeErrorKind::DivisionByZero)
        );
        assert_eq!(exec(&mut interp, "i"), Ok(Value::Int(3)));
    }

    #[test]
    fn function_does_not_see_caller_locals() {
        assert_eq!(run("fn f() { y } { let y = 1; f() }"), Err(undefined("y")));
    }

    #[test]
    fn call_before_declaration() {
        assert_eq!(
            run("f(); fn f() {} 0"),
            Err(RuntimeErrorKind::UndefinedFunction("f".to_string()))
        );
    }

    #[test]
    fn wrong_argument_count() {
        assert_eq!(
            run("fn add(a, b) { a + b } add(1)"),
            Err(RuntimeErrorKind::ArgumentCount {
                name: "add".to_string(),
                expected: 2,
                found: 1
            })
        );
    }

    #[test]
    fn call_non_function() {
        assert_eq!(
            run("let x = 1; x(2)"),
            Err(mismatch("function", Value::Int(1)))
        );
    }

    #[test]
    fn compare_functions() {
        assert!(matches!(
            value("fn f() {} f == f"),
            Err(RuntimeErrorKind::CannotCompare(..))
        ));
    }

    #[test]
    fn scopes_restored_after_error_in_function() {
        let mut interp = Interpreter::new(Vec::new());
        assert_eq!(exec(&mut interp, "fn f() { 1 / 0 }"), Ok(Value::Unit));
        assert_eq!(
            exec(&mut interp, "{ { f() } }"),
            Err(RuntimeErrorKind::DivisionByZero)
        );
        assert_eq!(exec(&mut interp, "let z = 1; z"), Ok(Value::Int(1)));
    }

    #[test]
    fn infinite_recursion() {
        with_big_stack(|| {
            assert_eq!(
                run("fn f() { f() } f()"),
                Err(RuntimeErrorKind::StackOverflow)
            );
        });
    }

    #[test]
    fn depth_restored_after_stack_overflow() {
        with_big_stack(|| {
            let mut interp = Interpreter::new(Vec::new());
            let program = "fn down(n) { if n == 1 { return 1; } down(n - 1) }";
            assert_eq!(exec(&mut interp, program), Ok(Value::Unit));
            assert_eq!(
                exec(&mut interp, "down(1001)"),
                Err(RuntimeErrorKind::StackOverflow)
            );
            assert_eq!(exec(&mut interp, "down(1000)"), Ok(Value::Int(1)));
        });
    }

    #[test]
    fn expression_statement_error() {
        assert_eq!(run("1 / 0; 2"), Err(RuntimeErrorKind::DivisionByZero));
    }

    #[test]
    fn undefined_function() {
        assert_eq!(
            run("foo(1)"),
            Err(RuntimeErrorKind::UndefinedFunction("foo".to_string()))
        );
    }

    #[test]
    fn print_wrong_argument_count() {
        assert_eq!(
            run("print(1, 2)"),
            Err(RuntimeErrorKind::ArgumentCount {
                name: "print".to_string(),
                expected: 1,
                found: 2
            })
        );
    }

    #[test]
    fn unit_in_arithmetic() {
        assert_eq!(run("print(1) + 1"), Err(mismatch("integer", Value::Unit)));
    }

    #[test]
    fn bool_in_arithmetic() {
        assert_eq!(run("true + 1"), Err(mismatch("integer", Value::Bool(true))));
    }

    #[test]
    fn negation_of_bool() {
        assert_eq!(run("-false"), Err(mismatch("integer", Value::Bool(false))));
    }

    #[test]
    fn compare_int_with_bool() {
        assert_eq!(
            value("1 == true"),
            Err(RuntimeErrorKind::CannotCompare(
                Value::Int(1),
                Value::Bool(true)
            ))
        );
    }

    #[test]
    fn ordering_of_bools() {
        assert_eq!(
            value("false < true"),
            Err(mismatch("integer", Value::Bool(false)))
        );
    }

    #[test]
    fn not_of_int() {
        assert_eq!(value("!1"), Err(mismatch("bool", Value::Int(1))));
    }

    #[test]
    fn and_with_int() {
        assert_eq!(value("1 && true"), Err(mismatch("bool", Value::Int(1))));
    }

    #[test]
    fn evaluated_right_operand_must_be_bool() {
        assert_eq!(
            value("true && print(1)"),
            Err(mismatch("bool", Value::Unit))
        );
    }

    #[test]
    fn division_by_zero() {
        assert_eq!(run("1 / 0"), Err(RuntimeErrorKind::DivisionByZero));
    }

    #[test]
    fn computed_division_by_zero() {
        assert_eq!(run("1 / (2 - 2)"), Err(RuntimeErrorKind::DivisionByZero));
    }

    #[test]
    fn addition_overflow() {
        assert_eq!(
            run("9223372036854775807 + 1"),
            Err(RuntimeErrorKind::IntegerOverflow)
        );
    }

    #[test]
    fn subtraction_overflow() {
        assert_eq!(
            run("0 - 9223372036854775807 - 2"),
            Err(RuntimeErrorKind::IntegerOverflow)
        );
    }

    #[test]
    fn multiplication_overflow() {
        assert_eq!(
            run("9223372036854775807 * 2"),
            Err(RuntimeErrorKind::IntegerOverflow)
        );
    }

    #[test]
    fn negation_overflow() {
        assert_eq!(
            run("-(-9223372036854775807 - 1)"),
            Err(RuntimeErrorKind::IntegerOverflow)
        );
    }

    #[test]
    fn negation_of_min_value_overflows() {
        assert_eq!(
            run("--9223372036854775808"),
            Err(RuntimeErrorKind::IntegerOverflow)
        );
    }

    #[test]
    fn min_value_divided_by_minus_one() {
        assert_eq!(
            run("-9223372036854775808 / -1"),
            Err(RuntimeErrorKind::IntegerOverflow)
        );
    }

    #[test]
    fn error_messages() {
        assert_eq!(undefined("x").to_string(), "undefined variable: x");
        assert_eq!(
            RuntimeErrorKind::UndefinedFunction("foo".to_string()).to_string(),
            "undefined function: foo"
        );
        assert_eq!(
            RuntimeErrorKind::ArgumentCount {
                name: "print".to_string(),
                expected: 1,
                found: 2
            }
            .to_string(),
            "print expects 1 argument, got 2"
        );
        assert_eq!(
            RuntimeErrorKind::ArgumentCount {
                name: "add".to_string(),
                expected: 2,
                found: 1
            }
            .to_string(),
            "add expects 2 arguments, got 1"
        );
        assert_eq!(
            mismatch("bool", Value::Unit).to_string(),
            "expected bool, got ()"
        );
        assert_eq!(
            RuntimeErrorKind::CannotCompare(Value::Int(1), Value::Bool(true)).to_string(),
            "cannot compare 1 with true"
        );
        assert_eq!(
            RuntimeErrorKind::DivisionByZero.to_string(),
            "division by zero"
        );
        assert_eq!(
            RuntimeErrorKind::IntegerOverflow.to_string(),
            "integer overflow"
        );
        assert_eq!(
            RuntimeErrorKind::StackOverflow.to_string(),
            "stack overflow: more than 1000 nested calls"
        );
        assert_eq!(
            RuntimeErrorKind::Output(io::ErrorKind::BrokenPipe).to_string(),
            "cannot write output: broken pipe"
        );
    }

    #[test]
    fn errors_are_located_at_the_failing_expression() {
        assert_eq!(error_at("let a = 1;\nb"), "2:1: undefined variable: b");
        assert_eq!(
            error_at("let a = 1;\n  b = 2;"),
            "2:3: undefined variable: b"
        );
        assert_eq!(error_at("1 +\n  true"), "1:3: expected integer, got true");
        assert_eq!(error_at("10 / (2 - 2)"), "1:4: division by zero");
        assert_eq!(error_at("!1"), "1:1: expected bool, got 1");
        assert_eq!(error_at("true && 1"), "1:6: expected bool, got 1");
        assert_eq!(error_at("if 1 { 2 }"), "1:4: expected bool, got 1");
        assert_eq!(error_at("while 0 {}"), "1:7: expected bool, got 0");
    }

    #[test]
    fn call_errors_are_located_at_the_call() {
        assert_eq!(error_at("  foo()"), "1:3: undefined function: foo");
        assert_eq!(error_at("let x = 1;\nx()"), "2:1: expected function, got 1");
        assert_eq!(
            error_at("fn f(a) {}\nf()"),
            "2:1: f expects 1 argument, got 0"
        );
        assert_eq!(
            error_at("print(1, 2)"),
            "1:1: print expects 1 argument, got 2"
        );
    }

    #[test]
    fn error_inside_function_is_located_in_its_body() {
        assert_eq!(
            error_at("fn f() {\n  1 / 0\n}\nf()"),
            "2:5: division by zero"
        );
    }

    #[test]
    fn stack_overflow_is_located_at_the_recursive_call() {
        with_big_stack(|| {
            assert_eq!(
                error_at("fn f() {\n  f()\n}\nf()"),
                "2:3: stack overflow: more than 1000 nested calls"
            );
        });
    }
}
