use std::io::Write;
use std::mem;
use std::rc::Rc;

use crate::chunk::{Bytecode, Function, Op};
use crate::runtime::{MAX_CALL_DEPTH, RuntimeError, RuntimeErrorKind, binary, slice, unary};
use crate::value::{Struct, Value};

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
                Op::Str(s) => self.stack.push(Value::Str(s)),
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
                    if !self.pop().as_bool() {
                        frame.ip = target;
                    }
                }
                Op::Function(index) => {
                    let function = Rc::clone(&self.functions[index]);
                    self.stack.push(Value::Function(function));
                }
                Op::Call(argc) => {
                    let args = self.stack.len() - argc;
                    let Value::Function(function) = &self.stack[args - 1] else {
                        panic!("the checker only lets functions be called");
                    };
                    let function = Rc::clone(function);
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
                Op::Len => {
                    let len = match self.pop() {
                        Value::Str(s) => s.len(),
                        Value::Array(items) => items.len(),
                        other => panic!("the checker rejects len of {other}"),
                    };
                    self.stack.push(Value::Int(len as i64));
                }
                Op::Bytes => {
                    let Value::Str(s) = self.pop() else {
                        panic!("the checker only lets bytes take a string");
                    };
                    let bytes = s.bytes().map(|b| Value::Int(i64::from(b))).collect();
                    self.stack.push(Value::Array(Rc::new(bytes)));
                }
                Op::Array(count) => {
                    let items = self.stack.split_off(self.stack.len() - count);
                    self.stack.push(Value::Array(Rc::new(items)));
                }
                Op::Struct { def, fields } => {
                    let values = self.stack.split_off(self.stack.len() - fields.len());
                    let mut ordered = vec![Value::Unit; fields.len()];
                    for (value, place) in values.into_iter().zip(fields) {
                        ordered[place] = value;
                    }
                    let value = Struct {
                        def,
                        fields: ordered,
                    };
                    self.stack.push(Value::Struct(Rc::new(value)));
                }
                Op::Index => {
                    let index = self.pop().as_int();
                    let Value::Array(items) = self.pop() else {
                        panic!("the checker only lets arrays be indexed");
                    };
                    let item = usize::try_from(index)
                        .ok()
                        .and_then(|i| items.get(i))
                        .ok_or_else(|| {
                            at(RuntimeErrorKind::IndexOutOfBounds {
                                index,
                                len: items.len(),
                            })
                        })?;
                    self.stack.push(item.clone());
                }
                Op::Slice { start, end } => {
                    let end = end.then(|| self.pop().as_int());
                    let start = start.then(|| self.pop().as_int());
                    let value = self.pop();
                    self.stack.push(slice(value, start, end).map_err(at)?);
                }
                Op::SetIndex { slot, depth } => {
                    let indexes = self.stack.split_off(self.stack.len() - depth);
                    let value = self.pop();
                    *self.place(frame.locals + slot, indexes).map_err(at)? = value;
                }
                Op::Push { slot, depth } => {
                    let value = self.pop();
                    let indexes = self.stack.split_off(self.stack.len() - depth);
                    let Value::Array(items) =
                        self.place(frame.locals + slot, indexes).map_err(at)?
                    else {
                        panic!("the checker only lets push change an array");
                    };
                    Rc::make_mut(items).push(value);
                    self.stack.push(Value::Unit);
                }
            }
        }
        Ok(self.stack.pop().unwrap_or(Value::Unit))
    }

    // Follows `indexes` from the local in `slot`. Copies every array on the
    // way that another value still shares, so a change through the result is
    // not seen through that value.
    fn place(&mut self, slot: usize, indexes: Vec<Value>) -> Result<&mut Value, RuntimeErrorKind> {
        let mut place = &mut self.locals[slot];
        for index in indexes {
            let index = index.as_int();
            let Value::Array(items) = place else {
                panic!("the checker only lets arrays be indexed");
            };
            let items = Rc::make_mut(items);
            let len = items.len();
            place = usize::try_from(index)
                .ok()
                .and_then(|i| items.get_mut(i))
                .ok_or(RuntimeErrorKind::IndexOutOfBounds { index, len })?;
        }
        Ok(place)
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
    use crate::checker::{Env, check};
    use crate::compiler::{Scope, compile};
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn run(input: &str) -> (Result<Value, RuntimeError>, String) {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        assert_eq!(check(&program, &Env::default()).errors, []);
        let bytecode = compile(&program, &Scope::default());
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

    fn session(lines: &[&str]) -> Vec<Result<Value, String>> {
        let mut vm = Vm::new(Vec::new());
        let mut env = Env::default();
        let mut scope = Scope::default();
        lines
            .iter()
            .map(|line| {
                let program = Parser::new(tokenize(line).unwrap()).parse().unwrap();
                let checked = check(&program, &env);
                if let Some(error) = checked.errors.first() {
                    return Err(error.to_string());
                }
                let bytecode = compile(&program, &scope);
                let value = vm.run(&bytecode).map_err(|e| e.to_string())?;
                env = checked.env;
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
        assert_eq!(vm("if false { 1; }"), Ok(Value::Unit));
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
        assert_eq!(vm("if true { 1; } 2"), Ok(Value::Int(2)));
    }

    #[test]
    fn top_level_variables() {
        assert_eq!(vm("let x = 2; let y = x * 3; y + 1"), Ok(Value::Int(7)));
        assert_eq!(vm("let mut x = 1; x = x + 10; x"), Ok(Value::Int(11)));
        assert_eq!(vm("let x = 1; let x = x + 1; x"), Ok(Value::Int(2)));
    }

    #[test]
    fn local_variables() {
        assert_eq!(vm("{ let a = 1; let b = 2; a + b }"), Ok(Value::Int(3)));
        assert_eq!(vm("{ let a = 1; { let b = 2; a + b } }"), Ok(Value::Int(3)));
        assert_eq!(vm("let x = 1; { let x = x + 1; x }"), Ok(Value::Int(2)));
        assert_eq!(
            vm("let x = 1; { let mut x = 2; x = 3; } x"),
            Ok(Value::Int(1))
        );
        assert_eq!(
            vm("let mut x = 1; { let y = 2; x = x + y; } x"),
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
            vm("let mut i = 0; let mut sum = 0; while i < 5 { i = i + 1; sum = sum + i; } sum"),
            Ok(Value::Int(15))
        );
        assert_eq!(vm("while false { 1; }"), Ok(Value::Unit));
        assert_eq!(
            vm("let mut i = 0; while i < 3 { let j = i; i = j + 1; } i"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn break_and_continue() {
        assert_eq!(
            vm("let mut i = 0; while true { i = i + 1; if i == 4 { break; } } i"),
            Ok(Value::Int(4))
        );
        assert_eq!(
            vm("let mut i = 0; let mut sum = 0;
                 while i < 5 { i = i + 1; if i == 3 { continue; } sum = sum + i; }
                 sum"),
            Ok(Value::Int(12))
        );
        assert_eq!(vm("while true { break; }"), Ok(Value::Unit));
    }

    #[test]
    fn break_leaves_only_inner_loop() {
        assert_eq!(
            vm("let mut i = 0; let mut count = 0;
                 while i < 3 {
                     i = i + 1;
                     let mut j = 0;
                     while true { j = j + 1; if j == 2 { break; } count = count + 1; }
                 }
                 count"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn break_and_continue_clean_up_stacks() {
        assert_eq!(vm("while true { 1 + break; };"), Ok(Value::Unit));
        assert_eq!(vm("while true { [1, 2, break]; };"), Ok(Value::Unit));
        assert_eq!(vm("while true { [1][break]; };"), Ok(Value::Unit));
        assert_eq!(vm("while true { [1][0..break]; };"), Ok(Value::Unit));
        assert_eq!(
            vm("let mut a = [[0]]; while true { a[0][break] = 1; } a[0][0]"),
            Ok(Value::Int(0))
        );
        assert_eq!(
            vm("let mut a = [[0]]; while true { push(a[0], break); };"),
            Ok(Value::Unit)
        );
        assert_eq!(
            vm("let mut i = 0; while i < 3 { i = i + 1; 2 * { let a = i; continue; }; };"),
            Ok(Value::Unit)
        );
        assert_eq!(
            vm("while true { let a = 1; { let b = 2; break; } } { let c = 3; c }"),
            Ok(Value::Int(3))
        );
        assert_eq!(
            vm("let mut i = 0; while i < 3 { let a = i; i = i + 1; continue; } { let c = 7; c }"),
            Ok(Value::Int(7))
        );
    }

    #[test]
    fn functions() {
        assert_eq!(
            vm("fn add(a: int, b: int) -> int { a + b } add(1, 2)"),
            Ok(Value::Int(3))
        );
        assert_eq!(vm("fn f() {} f()"), Ok(Value::Unit));
        assert_eq!(
            vm("fn fact(n: int) -> int { if n <= 1 { 1 } else { n * fact(n - 1) } } fact(10)"),
            Ok(Value::Int(3628800))
        );
        assert_eq!(
            vm("fn f(x: int) -> int { let y = x * 2; y + 1 } let a = 1; f(5) + a"),
            Ok(Value::Int(12))
        );
        assert_eq!(vm("fn f() {}"), Ok(Value::Unit));
        assert_eq!(
            vm("fn f() -> int { 1 } { let y = 2; f() + y }"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn mut_parameters() {
        assert_eq!(
            vm(
                "fn steps(mut n: int) -> int { let mut count = 0; while n > 1 { n = n / 2; count = count + 1; } count } steps(8)"
            ),
            Ok(Value::Int(3))
        );
        assert_eq!(
            vm("fn inc(mut n: int) -> int { n = n + 1; n } let x = 5; inc(x) + x"),
            Ok(Value::Int(11))
        );
    }

    #[test]
    fn parameters_and_locals_are_separate_from_outer_variables() {
        assert_eq!(
            vm("let x = 1; fn f(x: int) -> int { x } f(2) + x"),
            Ok(Value::Int(3))
        );
        assert_eq!(
            vm("let x = 1; fn f() -> int { let x = 2; x } f() + x"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn mutual_recursion() {
        assert_eq!(
            vm(
                "fn even(n: int) -> bool { if n == 0 { true } else { odd(n - 1) } }
                 fn odd(n: int) -> bool { if n == 0 { false } else { even(n - 1) } }
                 even(10)"
            ),
            Ok(Value::Bool(true))
        );
    }

    #[test]
    fn functions_are_visible_before_declaration() {
        assert_eq!(vm("let x = f(); fn f() -> int { 7 } x"), Ok(Value::Int(7)));
    }

    #[test]
    fn return_from_function() {
        assert_eq!(
            vm("fn abs(x: int) -> int { if x < 0 { return -x; } x } abs(-5) + abs(3)"),
            Ok(Value::Int(8))
        );
        assert_eq!(
            vm(
                "fn root(n: int) -> int { let mut i = 0; while true { if i * i >= n { return i; } i = i + 1; } 0 } root(50)"
            ),
            Ok(Value::Int(8))
        );
        assert_eq!(vm("fn f() { return; 1; } f()"), Ok(Value::Unit));
        assert_eq!(
            vm("fn sign(x: int) -> int { if x < 0 { return -1 } 1 } sign(-3)"),
            Ok(Value::Int(-1))
        );
        assert_eq!(
            vm("fn g() -> int { return 1; } fn f() -> int { g() + 1 } f()"),
            Ok(Value::Int(2))
        );
        assert_eq!(output("fn f() { print(1); return; print(2); } f();"), "1\n");
    }

    #[test]
    fn return_cleans_up_stacks() {
        assert_eq!(
            vm("fn f() -> int { 1 + return 2; } 10 + f()"),
            Ok(Value::Int(12))
        );
        assert_eq!(
            vm("fn f() -> int { let a = 1; { let b = 2; return a + b; } } let x = 5; f() + x"),
            Ok(Value::Int(8))
        );
        assert_eq!(
            vm("fn f() -> int { { { return 1; } } } { let y = 2; f() + y }"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn functions_are_values() {
        assert_eq!(
            vm("fn add(a: int, b: int) -> int { a + b } let plus = add; plus(1, 2)"),
            Ok(Value::Int(3))
        );
        assert_eq!(
            vm(
                "fn twice(f: fn(int) -> int, x: int) -> int { f(f(x)) } fn inc(x: int) -> int { x + 1 } twice(inc, 5)"
            ),
            Ok(Value::Int(7))
        );
        assert_eq!(
            output("fn add(a: int, b: int) -> int { a + b } print(add);"),
            "<fn add>\n"
        );
    }

    #[test]
    fn variables_shadow_functions() {
        assert_eq!(vm("fn f() -> int { 1 } let f = 2; f"), Ok(Value::Int(2)));
        assert_eq!(
            vm("fn f() -> int { 1 } fn g(f: int) -> int { f } g(5)"),
            Ok(Value::Int(5))
        );
    }

    #[test]
    fn deep_recursion_needs_no_big_stack() {
        assert_eq!(
            vm("fn down(n: int) -> int { if n == 1 { return 1; } down(n - 1) } down(1000)"),
            Ok(Value::Int(1))
        );
    }

    #[test]
    fn long_chains_need_no_big_stack() {
        let sum = format!("{}1", "1 + ".repeat(100_000));
        assert_eq!(vm(&sum), Ok(Value::Int(100_001)));
        let all = format!("{}true", "true && ".repeat(100_000));
        assert_eq!(vm(&all), Ok(Value::Bool(true)));
    }

    #[test]
    fn print() {
        assert_eq!(output("let x = 1 + 2 * 3; print(x);"), "7\n");
        assert_eq!(output("print(false); print(print(1));"), "false\n1\n()\n");
        assert_eq!(
            output("false && { print(1); true }; true || { print(2); true };"),
            ""
        );
        assert_eq!(
            output("let mut i = 0; while i < 3 { print(i); i = i + 1; }"),
            "0\n1\n2\n"
        );
        assert_eq!(
            output("fn hello(x: int) { print(x); } hello(1); hello(2);"),
            "1\n2\n"
        );
    }

    #[test]
    fn strings() {
        assert_eq!(vm(r#""Ro" + "ck""#), Ok(Value::Str("Rock".into())));
        assert_eq!(vm(r#""a" == "a""#), Ok(Value::Bool(true)));
        assert_eq!(vm(r#""a" + "b" != "ab""#), Ok(Value::Bool(false)));
        assert_eq!(
            output(
                r#"fn greet(name: str) -> str { "Hello, " + name + "!" }
                   let mut s = "";
                   let mut i = 0;
                   while i < 3 { s = s + "*"; i = i + 1; }
                   print(greet("Rock"));
                   print(s);
                   print("a\tb\n");"#
            ),
            "Hello, Rock!\n***\na\tb\n\n"
        );
    }

    #[test]
    fn strings_compare_by_character_codes() {
        assert_eq!(vm(r#""apple" < "banana""#), Ok(Value::Bool(true)));
        assert_eq!(vm(r#""Zebra" < "apple""#), Ok(Value::Bool(true)));
        assert_eq!(vm(r#""ab" > "a""#), Ok(Value::Bool(true)));
        assert_eq!(vm(r#""" < "a""#), Ok(Value::Bool(true)));
        assert_eq!(vm(r#""a" <= "a" && "a" >= "a""#), Ok(Value::Bool(true)));
    }

    #[test]
    fn len_counts_bytes() {
        assert_eq!(vm(r#"len("")"#), Ok(Value::Int(0)));
        assert_eq!(vm(r#"len("Rock")"#), Ok(Value::Int(4)));
        assert_eq!(vm(r#"len("a\n")"#), Ok(Value::Int(2)));
        assert_eq!(vm(r#"len("привет")"#), Ok(Value::Int(12)));
    }

    #[test]
    fn bytes() {
        assert_eq!(output(r#"print(bytes("Rock"));"#), "[82, 111, 99, 107]\n");
        assert_eq!(output(r#"print(bytes("ж\n"));"#), "[208, 182, 10]\n");
        assert_eq!(output(r#"print(bytes(""));"#), "[]\n");
        assert_eq!(
            vm(r#"len(bytes("привет")) == len("привет")"#),
            Ok(Value::Bool(true))
        );
    }

    #[test]
    fn arrays() {
        assert_eq!(vm("[10, 20, 30][1]"), Ok(Value::Int(20)));
        assert_eq!(vm("let a = [[1], [2, 3]]; a[1][1]"), Ok(Value::Int(3)));
        assert_eq!(vm("len([1, 2, 3])"), Ok(Value::Int(3)));
        assert_eq!(vm("[1, 2] == [1, 2]"), Ok(Value::Bool(true)));
        assert_eq!(vm("[[1], [2]] != [[1], [3]]"), Ok(Value::Bool(true)));
        assert_eq!(
            output(r#"print([1, 2]); print([["a, b"], ["c"]]); print([[true]]);"#),
            "[1, 2]\n[[\"a, b\"], [\"c\"]]\n[[true]]\n"
        );
    }

    #[test]
    fn index_assignment() {
        assert_eq!(
            output("let mut a = [[1, 2], [3]]; a[0][1] = 5; a[1] = [6, 7]; print(a);"),
            "[[1, 5], [6, 7]]\n"
        );
    }

    #[test]
    fn index_assignment_leaves_copies_unchanged() {
        assert_eq!(
            vm("let mut a = [1]; let b = a; a[0] = 2; b[0] * 10 + a[0]"),
            Ok(Value::Int(12))
        );
        assert_eq!(
            vm("let mut a = [[0]]; let inner = a[0]; a[0][0] = 1; inner[0] * 10 + a[0][0]"),
            Ok(Value::Int(1))
        );
        assert_eq!(
            vm("fn f(mut xs: [int]) -> int { xs[0] = 9; xs[0] } let a = [1]; f(a) * 10 + a[0]"),
            Ok(Value::Int(91))
        );
    }

    #[test]
    fn push() {
        assert_eq!(
            output("let mut a: [[int]] = []; push(a, []); push(a[0], 1); push(a, [2]); print(a);"),
            "[[1], [2]]\n"
        );
        assert_eq!(
            vm("let mut a = [1]; let b = a; push(a, 2); len(a) * 10 + len(b)"),
            Ok(Value::Int(21))
        );
    }

    #[test]
    fn slices() {
        assert_eq!(vm(r#""hello"[1..3]"#), Ok(Value::Str("el".into())));
        assert_eq!(
            vm(r#""hello"[..2] + "hello"[3..]"#),
            Ok(Value::Str("helo".into()))
        );
        assert_eq!(vm(r#""hello"[5..]"#), Ok(Value::Str("".into())));
        assert_eq!(vm(r#""привет"[2..6]"#), Ok(Value::Str("ри".into())));
        assert_eq!(
            output("let a = [1, 2, 3, 4]; print(a[1..3]); print(a[..0]); print(a[..]);"),
            "[2, 3]\n[]\n[1, 2, 3, 4]\n"
        );
        assert_eq!(vm("[[1, 2], [3]][..1][0][1..][0]"), Ok(Value::Int(2)));
    }

    #[test]
    fn slice_is_a_copy() {
        assert_eq!(
            vm("let mut a = [1, 2]; let b = a[..1]; a[0] = 5; b[0] * 10 + a[0]"),
            Ok(Value::Int(15))
        );
    }

    #[test]
    fn for_loops() {
        assert_eq!(output("for i in 0..3 { print(i); }"), "0\n1\n2\n");
        assert_eq!(output("for i in -2..0 { print(i); }"), "-2\n-1\n");
        assert_eq!(
            vm("let mut n = 0; for _ in 5..5 { n = n + 1; } for _ in 3..1 { n = n + 1; } n"),
            Ok(Value::Int(0))
        );
        assert_eq!(
            vm("let mut sum = 0; for i in 1..4 { for j in 0..i { sum = sum + j; } } sum"),
            Ok(Value::Int(4))
        );
    }

    #[test]
    fn for_bounds_are_evaluated_once() {
        assert_eq!(
            output("fn f(n: int) -> int { print(n); n } for _ in f(0)..f(2) {}"),
            "0\n2\n"
        );
        assert_eq!(
            vm(
                "let mut n = 3; let mut count = 0; for _ in 0..n { n = n - 1; count = count + 1; } count"
            ),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn break_and_continue_in_for() {
        assert_eq!(
            vm("let mut sum = 0;
                 for i in 0..10 { if i == 4 { break; } if i == 1 { continue; } sum = sum + i; }
                 sum"),
            Ok(Value::Int(5))
        );
        assert_eq!(vm("while true { for _ in 0..break {} }"), Ok(Value::Unit));
        assert_eq!(
            vm("for i in 0..3 { let a = i; continue; } { let c = 3; c }"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn for_does_not_overflow_at_the_end() {
        assert_eq!(
            output("let max = 9223372036854775807; for i in max - 2..max { print(i); }"),
            "9223372036854775805\n9223372036854775806\n"
        );
    }

    #[test]
    fn session_after_for() {
        assert_eq!(
            session(&[
                "for i in 0..2 { i; }",
                "for i in 0..2 { 1 / i; }",
                "let x = 5;",
                "x"
            ]),
            [
                Ok(Value::Unit),
                Err("1:19: division by zero".to_string()),
                Ok(Value::Unit),
                Ok(Value::Int(5)),
            ]
        );
    }

    #[test]
    fn for_over_arrays() {
        assert_eq!(
            output(r#"for w in ["a", "b"] { print(w); } for row in [[1], [2, 3]] { print(row); }"#),
            "a\nb\n[1]\n[2, 3]\n"
        );
        assert_eq!(
            output(r#"for b in bytes("hi") { print(b); }"#),
            "104\n105\n"
        );
        assert_eq!(
            vm("let a: [int] = []; let mut n = 0; for _ in a { n = n + 1; } n"),
            Ok(Value::Int(0))
        );
        assert_eq!(
            vm("let mut sum = 0;
                 for x in [1, 2, 3, 4, 5] { if x == 4 { break; } if x == 2 { continue; } sum = sum + x; }
                 sum"),
            Ok(Value::Int(4))
        );
        assert_eq!(
            vm("for x in [1, 2] { let a = x; continue; } { let c = 3; c }"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn for_iterates_over_a_copy() {
        assert_eq!(
            output(
                "let mut a = [1, 2]; for x in a { push(a, x * 10); a[0] = 0; print(x); } print(a);"
            ),
            "1\n2\n[0, 2, 10, 20]\n"
        );
    }

    #[test]
    fn remainder() {
        assert_eq!(vm("7 % 3"), Ok(Value::Int(1)));
        assert_eq!(vm("-7 % 3"), Ok(Value::Int(-1)));
        assert_eq!(vm("7 % -3"), Ok(Value::Int(1)));
        assert_eq!(vm("-7 % -3"), Ok(Value::Int(-1)));
        assert_eq!(vm("100 % 7 % 3"), Ok(Value::Int(2)));
        assert_eq!(
            vm("let a = -7; let b = 3; a / b * b + a % b == a"),
            Ok(Value::Bool(true))
        );
        assert_eq!(vm("-9223372036854775808 % -1"), Ok(Value::Int(0)));
    }

    #[test]
    fn remainder_by_zero() {
        assert_eq!(error("1 % 0"), "1:3: division by zero");
    }

    #[test]
    fn compound_assignment() {
        assert_eq!(
            vm("let mut x = 10; x += 5; x -= 3; x *= 2; x /= 5; x %= 3; x"),
            Ok(Value::Int(1))
        );
        assert_eq!(
            vm(r#"let mut s = "ab"; s += "c"; s == "abc""#),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            vm("let mut a = [[1, 2], [3, 4]]; a[1][0] -= a[0][1]; a[1][0]"),
            Ok(Value::Int(1))
        );
    }

    #[test]
    fn compound_assignment_reads_before_value() {
        assert_eq!(vm("let mut x = 1; x += { x = 5; 1 }; x"), Ok(Value::Int(2)));
        assert_eq!(
            output(
                "fn i() -> int { print(0); 1 } let mut a = [1, 2]; \
                 a[i()] += { print(1); 10 }; print(a);"
            ),
            "0\n1\n[1, 12]\n"
        );
    }

    #[test]
    fn loop_control_in_compound_assignment() {
        assert_eq!(
            vm("let mut a = [1]; while true { a[0] += break; } let z = 9; z"),
            Ok(Value::Int(9))
        );
        assert_eq!(
            output(
                "let mut a = [5]; for i in 0..3 { a[{ if i == 1 { continue; } 0 }] += i; } \
                 let z = 9; print(z); print(a);"
            ),
            "9\n[7]\n"
        );
    }

    #[test]
    fn compound_assignment_errors() {
        assert_eq!(error("let mut x = 1; x /= 0; x"), "1:18: division by zero");
        assert_eq!(
            error("let mut a = [1]; a[0] %= 0; a"),
            "1:23: division by zero"
        );
        assert_eq!(
            error("let mut a = [1]; a[1] += 1; a"),
            "1:19: index 1 out of bounds for length 1"
        );
    }

    #[test]
    fn session_after_compound_assignment_error() {
        assert_eq!(
            session(&[
                "let mut a = [1];",
                "a[0] /= 0;",
                "a[0] += 1;",
                "let x = 5;",
                "x + a[0]"
            ]),
            [
                Ok(Value::Unit),
                Err("1:6: division by zero".to_string()),
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Int(7)),
            ]
        );
    }

    #[test]
    fn structs() {
        assert_eq!(
            output(
                r#"struct P { x: int, y: int } struct S { name: str, points: [P], tag: E }
                   struct E {}
                   print(S { points: [P { y: 2, x: 1 }], tag: E {}, name: "a, b" });"#
            ),
            "S { name: \"a, b\", points: [P { x: 1, y: 2 }], tag: E {} }\n"
        );
        assert_eq!(
            vm(r#"struct P { x: int, s: [str] } P { x: 1, s: ["a"] } == P { s: ["a"], x: 1 }"#),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            vm(
                "struct P { x: int } fn f(p: P) -> [P] { [p, p] } f(P { x: 1 }) == [P { x: 1 }, P { x: 2 }]"
            ),
            Ok(Value::Bool(false))
        );
        assert_eq!(
            output("struct P { x: int } for p in [P { x: 1 }, P { x: 2 }] { print(p); }"),
            "P { x: 1 }\nP { x: 2 }\n"
        );
    }

    #[test]
    fn struct_fields_are_evaluated_in_written_order() {
        assert_eq!(
            output(
                "struct P { x: int, y: int } print(P { y: { print(1); 1 }, x: { print(2); 2 } });"
            ),
            "1\n2\nP { x: 2, y: 1 }\n"
        );
    }

    #[test]
    fn session_keeps_structs() {
        assert_eq!(
            session(&[
                "struct P { x: int }",
                "let p = P { x: 1 };",
                "struct P { y: int }",
                "fn f(q: P) -> P { q }",
                "f(p) == P { x: 1 }"
            ]),
            [
                Ok(Value::Unit),
                Ok(Value::Unit),
                Err("1:8: duplicate struct: P".to_string()),
                Ok(Value::Unit),
                Ok(Value::Bool(true)),
            ]
        );
    }

    #[test]
    fn functions_shadow_print() {
        assert_eq!(output("fn print(x: int) -> int { x } print(1);"), "");
    }

    #[test]
    fn session_keeps_variables_and_functions() {
        assert_eq!(
            session(&[
                "let mut x = 1;",
                "fn inc(n: int) -> int { n + 1 }",
                "x = inc(x);",
                "inc(x)"
            ]),
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
                "fn f() -> int { 1 }",
                "fn g() -> int { f() }",
                "fn f() -> int { 2 }",
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
    fn session_keeps_types() {
        assert_eq!(
            session(&[
                "fn f() -> int { 1 }",
                "fn g() -> int { f() }",
                "let x = f() == 1;",
                "fn f() -> bool { true }",
                "g() + 1",
                "x + 1",
                "f() + 1",
            ]),
            [
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Unit),
                Ok(Value::Int(2)),
                Err("1:1: expected int, found bool".to_string()),
                Err("1:1: expected int, found bool".to_string()),
            ]
        );
    }

    #[test]
    fn session_keeps_mutability() {
        assert_eq!(
            session(&["let x = 1;", "let mut y = 2;", "x = 3;", "y = 4;", "y"]),
            [
                Ok(Value::Unit),
                Ok(Value::Unit),
                Err("1:1: cannot assign to immutable variable: x".to_string()),
                Ok(Value::Unit),
                Ok(Value::Int(4)),
            ]
        );
    }

    #[test]
    fn errors() {
        assert_eq!(error("10 / (2 - 2)"), "1:4: division by zero");
        assert_eq!(error("9223372036854775807 + 1"), "1:21: integer overflow");
        assert_eq!(error("{ 1 / 0; 2 }"), "1:5: division by zero");
        assert_eq!(
            error("fn f() -> int {\n  1 / 0\n}\nf()"),
            "2:5: division by zero"
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
            vm("fn down(n: int) -> int { if n == 1 { return 1; } down(n - 1) } down(1001)")
                .unwrap_err()
                .to_string(),
            "1:50: stack overflow: more than 1000 nested calls"
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

    #[test]
    fn index_out_of_bounds() {
        assert_eq!(
            error("[1, 2, 3][3]"),
            "1:10: index 3 out of bounds for length 3"
        );
        assert_eq!(
            error("let a = [1];\na[0 - 1]"),
            "2:2: index -1 out of bounds for length 1"
        );
    }

    #[test]
    fn index_assignment_out_of_bounds() {
        assert_eq!(
            error("let mut a = [1];\na[1] = 2;"),
            "2:1: index 1 out of bounds for length 1"
        );
        assert_eq!(
            error("let mut a = [[1]];\na[0][0 - 1] = 2;"),
            "2:1: index -1 out of bounds for length 1"
        );
    }

    #[test]
    fn push_out_of_bounds() {
        assert_eq!(
            error("let mut a = [[1]];\npush(a[1], 2);"),
            "2:1: index 1 out of bounds for length 1"
        );
    }

    #[test]
    fn slice_out_of_bounds() {
        assert_eq!(
            error("[1, 2][1..3]"),
            "1:7: slice index 3 out of bounds for length 2"
        );
        assert_eq!(
            error("let s = \"ab\";\ns[0 - 1..]"),
            "2:2: slice index -1 out of bounds for length 2"
        );
        assert_eq!(
            error("[1, 2][2..1]"),
            "1:7: slice starts at 2 but ends at 1"
        );
    }

    #[test]
    fn slice_inside_char() {
        assert_eq!(
            error(r#""привет"[1..]"#),
            "1:9: byte index 1 is not a char boundary"
        );
        assert_eq!(
            error(r#""aж"[..2]"#),
            "1:5: byte index 2 is not a char boundary"
        );
    }
}
