use std::io::Write;
use std::mem;
use std::rc::Rc;

use crate::chunk::{Bytecode, Function, Op};
use crate::runtime::{MAX_CALL_DEPTH, RuntimeError, RuntimeErrorKind, binary, boolean, unary};
use crate::value::Value;

pub struct Vm<W: Write> {
    out: W,
    stack: Vec<Value>,
    locals: Vec<Value>,
    frames: Vec<Frame>,
    functions: Vec<Rc<Function>>,
}

struct Frame {
    function: Rc<Function>,
    ip: usize,
    locals: usize,
    stack: usize,
}

impl<W: Write> Vm<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            stack: Vec::new(),
            locals: Vec::new(),
            frames: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn run(&mut self, bytecode: &Bytecode) -> Result<Value, RuntimeError> {
        let locals = self.locals.len();
        let functions = self.functions.len();
        self.functions.extend(bytecode.functions.iter().cloned());
        let result = self.execute(&bytecode.main);
        if result.is_err() {
            self.stack.clear();
            self.frames.clear();
            self.locals.truncate(locals);
            self.functions.truncate(functions);
        }
        result
    }

    fn execute(&mut self, main: &Rc<Function>) -> Result<Value, RuntimeError> {
        let mut frame = Frame {
            function: Rc::clone(main),
            ip: 0,
            locals: 0,
            stack: 0,
        };
        while let Some(op) = frame.function.chunk.code.get(frame.ip).cloned() {
            let pos = frame.function.chunk.positions[frame.ip];
            let at = |kind: RuntimeErrorKind| kind.at(pos);
            frame.ip += 1;
            match op {
                Op::Int(n) => self.stack.push(Value::Int(n)),
                Op::Bool(b) => self.stack.push(Value::Bool(b)),
                Op::Unit => self.stack.push(Value::Unit),
                Op::Pop => {
                    self.pop();
                }
                Op::DefineLocal => {
                    let value = self.pop();
                    self.locals.push(value);
                }
                Op::GetLocal(slot) => self.stack.push(self.locals[frame.locals + slot].clone()),
                Op::SetLocal(slot) => self.locals[frame.locals + slot] = self.pop(),
                Op::DropLocals(count) => self.locals.truncate(self.locals.len() - count),
                Op::Unary(op) => {
                    let value = self.pop();
                    self.stack.push(unary(&op, value).map_err(at)?);
                }
                Op::Binary(op) => {
                    let r = self.pop();
                    let l = self.pop();
                    self.stack.push(binary(&op, l, r).map_err(at)?);
                }
                Op::Jump(target) => frame.ip = target,
                Op::JumpIfFalse(target) => {
                    if !boolean(self.pop()).map_err(at)? {
                        frame.ip = target;
                    }
                }
                Op::Function(index) => {
                    let function = Rc::clone(&self.functions[index]);
                    self.stack.push(Value::Function(function));
                }
                Op::Call(argc) => {
                    let args = self.stack.len() - argc;
                    let function = match &self.stack[args - 1] {
                        Value::Function(function) => Rc::clone(function),
                        found => {
                            return Err(at(RuntimeErrorKind::TypeMismatch {
                                expected: "function",
                                found: found.clone(),
                            }));
                        }
                    };
                    if argc != function.arity {
                        return Err(at(RuntimeErrorKind::ArgumentCount {
                            name: function.name.clone(),
                            expected: function.arity,
                            found: argc,
                        }));
                    }
                    if self.frames.len() == MAX_CALL_DEPTH {
                        return Err(at(RuntimeErrorKind::StackOverflow));
                    }
                    let locals = self.locals.len();
                    self.locals.extend(self.stack.drain(args..));
                    self.pop();
                    let callee = Frame {
                        function,
                        ip: 0,
                        locals,
                        stack: self.stack.len(),
                    };
                    self.frames.push(mem::replace(&mut frame, callee));
                }
                Op::Return => {
                    let value = self.pop();
                    self.stack.truncate(frame.stack);
                    self.locals.truncate(frame.locals);
                    frame = self.frames.pop().expect("main never returns");
                    self.stack.push(value);
                }
                Op::Print => {
                    let value = self.pop();
                    writeln!(self.out, "{value}")
                        .map_err(|e| at(RuntimeErrorKind::Output(e.kind())))?;
                    self.stack.push(Value::Unit);
                }
            }
        }
        Ok(self.stack.pop().unwrap_or(Value::Unit))
    }

    fn pop(&mut self) -> Value {
        self.stack
            .pop()
            .expect("the compiler keeps the stack balanced")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{Scope, compile};
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn run(input: &str) -> (Result<Value, RuntimeError>, String) {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        let bytecode = compile(&program, &Scope::default()).unwrap();
        let mut out = Vec::new();
        let result = Vm::new(&mut out).run(&bytecode);
        (result, String::from_utf8(out).unwrap())
    }

    fn vm(input: &str) -> Result<Value, RuntimeError> {
        run(input).0
    }

    fn output(input: &str) -> String {
        run(input).1
    }

    fn error(input: &str) -> String {
        vm(input).unwrap_err().to_string()
    }

    fn compile_error(input: &str) -> String {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        compile(&program, &Scope::default())
            .unwrap_err()
            .to_string()
    }

    fn session(lines: &[&str]) -> Vec<Result<Value, String>> {
        let mut vm = Vm::new(Vec::new());
        let mut scope = Scope::default();
        lines
            .iter()
            .map(|line| {
                let program = Parser::new(tokenize(line).unwrap()).parse().unwrap();
                let bytecode = compile(&program, &scope).map_err(|e| e.to_string())?;
                let value = vm.run(&bytecode).map_err(|e| e.to_string())?;
                scope = bytecode.scope;
                Ok(value)
            })
            .collect()
    }

    #[test]
    fn arithmetic() {
        assert_eq!(vm("1 + 2 * 3"), Ok(Value::Int(7)));
        assert_eq!(vm("-(4 - 10) / 2"), Ok(Value::Int(3)));
        assert_eq!(vm("10 - 3 - 2"), Ok(Value::Int(5)));
        assert_eq!(vm("-9223372036854775808"), Ok(Value::Int(i64::MIN)));
        assert_eq!(vm("-9223372036854775807 - 1"), Ok(Value::Int(i64::MIN)));
        assert_eq!(vm("7 / 2"), Ok(Value::Int(3)));
        assert_eq!(vm("--5"), Ok(Value::Int(5)));
    }

    #[test]
    fn comparisons() {
        assert_eq!(vm("1 + 1 == 2"), Ok(Value::Bool(true)));
        assert_eq!(vm("3 <= 2"), Ok(Value::Bool(false)));
        assert_eq!(vm("true != false"), Ok(Value::Bool(true)));
        assert_eq!(vm("1 < 2"), Ok(Value::Bool(true)));
        assert_eq!(vm("1 > 2"), Ok(Value::Bool(false)));
        assert_eq!(vm("2 >= 2"), Ok(Value::Bool(true)));
        assert_eq!(vm("print(1) == print(2)"), Ok(Value::Bool(true)));
    }

    #[test]
    fn logical_operators() {
        assert_eq!(vm("!true"), Ok(Value::Bool(false)));
        for (input, expected) in [
            ("true && true", true),
            ("true && false", false),
            ("false && true", false),
            ("false && false", false),
            ("true || true", true),
            ("true || false", true),
            ("false || true", true),
            ("false || false", false),
        ] {
            assert_eq!(vm(input), Ok(Value::Bool(expected)), "{input}");
        }
    }

    #[test]
    fn logical_operators_short_circuit() {
        assert_eq!(vm("false && 1 / 0 == 0"), Ok(Value::Bool(false)));
        assert_eq!(vm("true || 1 / 0 == 0"), Ok(Value::Bool(true)));
    }

    #[test]
    fn if_else() {
        assert_eq!(vm("if 1 < 2 { 10 } else { 20 }"), Ok(Value::Int(10)));
        assert_eq!(vm("if 1 > 2 { 10 } else { 20 }"), Ok(Value::Int(20)));
        assert_eq!(
            vm("if false { 1 } else if false { 2 } else { 3 }"),
            Ok(Value::Int(3))
        );
        assert_eq!(vm("if false { 1 }"), Ok(Value::Unit));
        assert_eq!(vm("if false { 1 / 0 } else { 2 }"), Ok(Value::Int(2)));
        assert_eq!(output("if true { print(1); } else { print(2); }"), "1\n");
    }

    #[test]
    fn blocks_and_statements() {
        assert_eq!(vm("{ 1; 2; 3 }"), Ok(Value::Int(3)));
        assert_eq!(vm("{ 1 + 2; }"), Ok(Value::Unit));
        assert_eq!(vm("{}"), Ok(Value::Unit));
        assert_eq!(vm("1; 2;"), Ok(Value::Unit));
        assert_eq!(vm(""), Ok(Value::Unit));
        assert_eq!(vm("if true { 1 } 2"), Ok(Value::Int(2)));
    }

    #[test]
    fn top_level_variables() {
        assert_eq!(vm("let x = 2; let y = x * 3; y + 1"), Ok(Value::Int(7)));
        assert_eq!(vm("let x = 1; x = x + 10; x"), Ok(Value::Int(11)));
        assert_eq!(vm("let x = 1; let x = x + 1; x"), Ok(Value::Int(2)));
    }

    #[test]
    fn local_variables() {
        assert_eq!(vm("{ let a = 1; let b = 2; a + b }"), Ok(Value::Int(3)));
        assert_eq!(vm("{ let a = 1; { let b = 2; a + b } }"), Ok(Value::Int(3)));
        assert_eq!(vm("let x = 1; { let x = x + 1; x }"), Ok(Value::Int(2)));
        assert_eq!(vm("let x = 1; { let x = 2; x = 3; } x"), Ok(Value::Int(1)));
        assert_eq!(
            vm("let x = 1; { let y = 2; x = x + y; } x"),
            Ok(Value::Int(3))
        );
        assert_eq!(
            vm("let x = 1; let y = { let x = 2; x }; x + y"),
            Ok(Value::Int(3))
        );
        assert_eq!(vm("let x = 1; if true { let x = 2; } x"), Ok(Value::Int(1)));
    }

    #[test]
    fn locals_below_temporaries() {
        assert_eq!(vm("1 + { let a = 2; let b = 3; a * b }"), Ok(Value::Int(7)));
        assert_eq!(
            vm("let x = 5; if x > 3 { let y = x * 2; y } else { 0 }"),
            Ok(Value::Int(10))
        );
    }

    #[test]
    fn locals_dropped_after_block() {
        assert_eq!(
            vm("{ let a = 1; } { let b = 2; } { let c = 3; c }"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn while_loops() {
        assert_eq!(
            vm("let i = 0; let sum = 0; while i < 5 { i = i + 1; sum = sum + i; } sum"),
            Ok(Value::Int(15))
        );
        assert_eq!(vm("while false { 1 }"), Ok(Value::Unit));
        assert_eq!(
            vm("let i = 0; while i < 3 { let j = i; i = j + 1; } i"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn break_and_continue() {
        assert_eq!(
            vm("let i = 0; while true { i = i + 1; if i == 4 { break; } } i"),
            Ok(Value::Int(4))
        );
        assert_eq!(
            vm("let i = 0; let sum = 0;
                 while i < 5 { i = i + 1; if i == 3 { continue; } sum = sum + i; }
                 sum"),
            Ok(Value::Int(12))
        );
        assert_eq!(vm("while true { break; }"), Ok(Value::Unit));
    }

    #[test]
    fn break_leaves_only_inner_loop() {
        assert_eq!(
            vm("let i = 0; let count = 0;
                 while i < 3 {
                     i = i + 1;
                     let j = 0;
                     while true { j = j + 1; if j == 2 { break; } count = count + 1; }
                 }
                 count"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn break_and_continue_clean_up_stacks() {
        assert_eq!(vm("while true { 1 + break; };"), Ok(Value::Unit));
        assert_eq!(
            vm("let i = 0; while i < 3 { i = i + 1; 2 * { let a = i; continue; }; };"),
            Ok(Value::Unit)
        );
        assert_eq!(
            vm("while true { let a = 1; { let b = 2; break; } } { let c = 3; c }"),
            Ok(Value::Int(3))
        );
        assert_eq!(
            vm("let i = 0; while i < 3 { let a = i; i = i + 1; continue; } { let c = 7; c }"),
            Ok(Value::Int(7))
        );
    }

    #[test]
    fn functions() {
        assert_eq!(vm("fn add(a, b) { a + b } add(1, 2)"), Ok(Value::Int(3)));
        assert_eq!(vm("fn f() {} f()"), Ok(Value::Unit));
        assert_eq!(
            vm("fn fact(n) { if n <= 1 { 1 } else { n * fact(n - 1) } } fact(10)"),
            Ok(Value::Int(3628800))
        );
        assert_eq!(
            vm("fn f(x) { let y = x * 2; y + 1 } let a = 1; f(5) + a"),
            Ok(Value::Int(12))
        );
        assert_eq!(vm("fn f() {}"), Ok(Value::Unit));
        assert_eq!(vm("fn f() { 1 } { let y = 2; f() + y }"), Ok(Value::Int(3)));
    }

    #[test]
    fn parameters_and_locals_are_separate_from_outer_variables() {
        assert_eq!(vm("let x = 1; fn f(x) { x } f(2) + x"), Ok(Value::Int(3)));
        assert_eq!(
            vm("let x = 1; fn f() { let x = 2; x } f() + x"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn mutual_recursion() {
        assert_eq!(
            vm("fn even(n) { if n == 0 { true } else { odd(n - 1) } }
                 fn odd(n) { if n == 0 { false } else { even(n - 1) } }
                 even(10)"),
            Ok(Value::Bool(true))
        );
    }

    #[test]
    fn functions_are_visible_before_declaration() {
        assert_eq!(vm("let x = f(); fn f() { 7 } x"), Ok(Value::Int(7)));
    }

    #[test]
    fn return_from_function() {
        assert_eq!(
            vm("fn abs(x) { if x < 0 { return -x; } x } abs(-5) + abs(3)"),
            Ok(Value::Int(8))
        );
        assert_eq!(
            vm(
                "fn root(n) { let i = 0; while true { if i * i >= n { return i; } i = i + 1; } } root(50)"
            ),
            Ok(Value::Int(8))
        );
        assert_eq!(vm("fn f() { return; 1 } f()"), Ok(Value::Unit));
        assert_eq!(
            vm("fn sign(x) { if x < 0 { return -1 } 1 } sign(-3)"),
            Ok(Value::Int(-1))
        );
        assert_eq!(
            vm("fn g() { return 1; } fn f() { g() + 1 } f()"),
            Ok(Value::Int(2))
        );
        assert_eq!(output("fn f() { print(1); return; print(2); } f();"), "1\n");
    }

    #[test]
    fn return_cleans_up_stacks() {
        assert_eq!(vm("fn f() { 1 + return 2; } 10 + f()"), Ok(Value::Int(12)));
        assert_eq!(
            vm("fn f() { let a = 1; { let b = 2; return a + b; } } let x = 5; f() + x"),
            Ok(Value::Int(8))
        );
        assert_eq!(
            vm("fn f() { { { return 1; } } } { let y = 2; f() + y }"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn functions_are_values() {
        assert_eq!(
            vm("fn add(a, b) { a + b } let plus = add; plus(1, 2)"),
            Ok(Value::Int(3))
        );
        assert_eq!(
            vm("fn twice(f, x) { f(f(x)) } fn inc(x) { x + 1 } twice(inc, 5)"),
            Ok(Value::Int(7))
        );
        assert_eq!(output("fn add(a, b) { a + b } print(add);"), "<fn add>\n");
    }

    #[test]
    fn variables_shadow_functions() {
        assert_eq!(vm("fn f() { 1 } let f = 2; f"), Ok(Value::Int(2)));
        assert_eq!(vm("fn f() { 1 } fn g(f) { f } g(5)"), Ok(Value::Int(5)));
    }

    #[test]
    fn deep_recursion_needs_no_big_stack() {
        assert_eq!(
            vm("fn down(n) { if n == 1 { return 1; } down(n - 1) } down(1000)"),
            Ok(Value::Int(1))
        );
    }

    #[test]
    fn print() {
        assert_eq!(output("let x = 1 + 2 * 3; print(x);"), "7\n");
        assert_eq!(output("print(false); print(print(1));"), "false\n1\n()\n");
        assert_eq!(output("false && print(1); true || print(2);"), "");
        assert_eq!(
            output("let i = 0; while i < 3 { print(i); i = i + 1; }"),
            "0\n1\n2\n"
        );
        assert_eq!(
            output("fn hello(x) { print(x); } hello(1); hello(2);"),
            "1\n2\n"
        );
    }

    #[test]
    fn functions_shadow_print() {
        assert_eq!(output("fn print(x) { x } print(1);"), "");
    }

    #[test]
    fn session_keeps_variables_and_functions() {
        assert_eq!(
            session(&["let x = 1;", "fn inc(n) { n + 1 }", "x = inc(x);", "inc(x)"]),
            [
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Int(3))
            ]
        );
    }

    #[test]
    fn session_redefines_functions() {
        assert_eq!(
            session(&[
                "fn f() { 1 }",
                "fn g() { f() }",
                "fn f() { 2 }",
                "f() * 10 + g()"
            ]),
            [
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Int(21))
            ]
        );
    }

    #[test]
    fn errors() {
        assert_eq!(error("1 +\n  true"), "1:3: expected integer, got true");
        assert_eq!(error("10 / (2 - 2)"), "1:4: division by zero");
        assert_eq!(error("9223372036854775807 + 1"), "1:21: integer overflow");
        assert_eq!(error("!1"), "1:1: expected bool, got 1");
        assert_eq!(error("1 == true"), "1:3: cannot compare 1 with true");
        assert_eq!(error("true && 1"), "1:6: expected bool, got 1");
        assert_eq!(error("1 || true"), "1:3: expected bool, got 1");
        assert_eq!(error("if 1 { 2 }"), "1:4: expected bool, got 1");
        assert_eq!(error("{ 1 / 0; 2 }"), "1:5: division by zero");
        assert_eq!(error("while 1 { 2 }"), "1:7: expected bool, got 1");
        assert_eq!(error("print(1) + 1"), "1:10: expected integer, got ()");
        assert_eq!(error("-false"), "1:1: expected integer, got false");
        assert_eq!(error("false < true"), "1:7: expected integer, got false");
        assert_eq!(error("true && print(1)"), "1:6: expected bool, got ()");
        assert_eq!(
            vm("fn f() {} f == f").unwrap_err().to_string(),
            "1:13: cannot compare <fn f> with <fn f>"
        );
    }

    #[test]
    fn integer_overflow() {
        assert_eq!(
            error("0 - 9223372036854775807 - 2"),
            "1:25: integer overflow"
        );
        assert_eq!(error("9223372036854775807 * 2"), "1:21: integer overflow");
        assert_eq!(
            error("-(-9223372036854775807 - 1)"),
            "1:1: integer overflow"
        );
        assert_eq!(error("--9223372036854775808"), "1:1: integer overflow");
        assert_eq!(error("-9223372036854775808 / -1"), "1:22: integer overflow");
    }

    #[test]
    fn call_errors() {
        assert_eq!(
            error("fn add(a, b) { a + b } add(1)"),
            "1:24: add expects 2 arguments, got 1"
        );
        assert_eq!(error("let f = 2; f()"), "1:12: expected function, got 2");
        assert_eq!(
            error("let print = 1; print(2)"),
            "1:16: expected function, got 1"
        );
        assert_eq!(error("fn f() {\n  1 / 0\n}\nf()"), "2:5: division by zero");
    }

    #[test]
    fn output_before_error_is_kept() {
        let (result, out) = run("print(1); print(1 / 0);");
        assert_eq!(result.unwrap_err().to_string(), "1:19: division by zero");
        assert_eq!(out, "1\n");
    }

    #[test]
    fn stack_overflow() {
        assert_eq!(
            vm("fn f() { f() } f()").unwrap_err().to_string(),
            "1:10: stack overflow: more than 1000 nested calls"
        );
        assert_eq!(
            vm("fn f() {\n  f()\n}\nf()").unwrap_err().to_string(),
            "2:3: stack overflow: more than 1000 nested calls"
        );
        assert_eq!(
            vm("fn down(n) { if n == 1 { return 1; } down(n - 1) } down(1001)")
                .unwrap_err()
                .to_string(),
            "1:38: stack overflow: more than 1000 nested calls"
        );
    }

    #[test]
    fn runtime_error_through_variable() {
        assert_eq!(
            error("let x = true;\nx + 1"),
            "2:3: expected integer, got true"
        );
    }

    #[test]
    fn undefined_variables_are_compile_errors() {
        assert_eq!(compile_error("x"), "1:1: undefined variable: x");
        assert_eq!(compile_error("y = 1;"), "1:1: undefined variable: y");
        assert_eq!(
            compile_error("{ let a = 1; } a"),
            "1:16: undefined variable: a"
        );
        assert_eq!(compile_error("let x = x;"), "1:9: undefined variable: x");
        assert_eq!(
            compile_error("let y = x; let x = 1; y"),
            "1:9: undefined variable: x"
        );
    }

    #[test]
    fn functions_do_not_see_outer_variables() {
        assert_eq!(
            compile_error("let base = 10; fn f(x) { base + x } f(1)"),
            "1:26: undefined variable: base"
        );
        assert_eq!(
            compile_error("fn f() { y } { let y = 1; f() }"),
            "1:10: undefined variable: y"
        );
        assert_eq!(
            compile_error("let count = 0; fn reset() { count = 0; }"),
            "1:29: undefined variable: count"
        );
    }

    #[test]
    fn function_compile_errors() {
        assert_eq!(compile_error("f()"), "1:1: undefined function: f");
        assert_eq!(
            compile_error("fn f() {} fn f() {}"),
            "1:14: duplicate function: f"
        );
        assert_eq!(
            compile_error("fn f() {} f = 1;"),
            "1:11: undefined variable: f"
        );
        assert_eq!(
            compile_error("print(1, 2)"),
            "1:1: print expects 1 argument, got 2"
        );
    }

    #[test]
    fn undefined_variable_in_unreached_code_is_an_error() {
        assert_eq!(
            compile_error("if false { y }"),
            "1:12: undefined variable: y"
        );
    }

    #[test]
    fn session_rolls_back_failed_lines() {
        assert_eq!(
            session(&[
                "let x = 1;",
                "let y = 2; 1 / 0",
                "y",
                "let z = 3; w",
                "z",
                "fn f() { f() } f()",
                "f",
                "let v = 5;",
                "v + x",
            ]),
            [
                Ok(Value::Unit),
                Err("1:14: division by zero".to_string()),
                Err("1:1: undefined variable: y".to_string()),
                Err("1:12: undefined variable: w".to_string()),
                Err("1:1: undefined variable: z".to_string()),
                Err("1:10: stack overflow: more than 1000 nested calls".to_string()),
                Err("1:1: undefined variable: f".to_string()),
                Ok(Value::Unit),
                Ok(Value::Int(6)),
            ]
        );
    }
}
