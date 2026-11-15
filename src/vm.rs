use crate::chunk::{Chunk, Op};
use crate::interp::{RuntimeError, RuntimeErrorKind, binary, boolean, unary};
use crate::value::Value;

pub struct Vm {
    stack: Vec<Value>,
    globals: Vec<Value>,
    locals: Vec<Value>,
}

impl Vm {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            globals: Vec::new(),
            locals: Vec::new(),
        }
    }

    pub fn run(&mut self, chunk: &Chunk) -> Result<Value, RuntimeError> {
        let mut ip = 0;
        while let Some(op) = chunk.code.get(ip) {
            let pos = chunk.positions[ip];
            let at = |kind: RuntimeErrorKind| kind.at(pos);
            ip += 1;
            match op {
                Op::Int(n) => self.stack.push(Value::Int(*n)),
                Op::Bool(b) => self.stack.push(Value::Bool(*b)),
                Op::Unit => self.stack.push(Value::Unit),
                Op::Pop => {
                    self.pop();
                }
                Op::DefineGlobal => {
                    let value = self.pop();
                    self.globals.push(value);
                }
                Op::GetGlobal(slot) => self.stack.push(self.globals[*slot].clone()),
                Op::SetGlobal(slot) => self.globals[*slot] = self.pop(),
                Op::DefineLocal => {
                    let value = self.pop();
                    self.locals.push(value);
                }
                Op::GetLocal(slot) => self.stack.push(self.locals[*slot].clone()),
                Op::SetLocal(slot) => self.locals[*slot] = self.pop(),
                Op::DropLocals(count) => self.locals.truncate(self.locals.len() - count),
                Op::Unary(op) => {
                    let value = self.pop();
                    self.stack.push(unary(op, value).map_err(at)?);
                }
                Op::Binary(op) => {
                    let r = self.pop();
                    let l = self.pop();
                    self.stack.push(binary(op, l, r).map_err(at)?);
                }
                Op::Jump(target) => ip = *target,
                Op::JumpIfFalse(target) => {
                    if !boolean(self.pop()).map_err(at)? {
                        ip = *target;
                    }
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
    use crate::compiler::compile;
    use crate::interp::Interpreter;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    // Runs the program on both the VM and the tree-walking interpreter
    // and checks that they agree, including error positions.
    fn agree(input: &str) -> Result<Value, RuntimeError> {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        let expected = Interpreter::new(Vec::new()).run(&program);
        let actual = Vm::new().run(&compile(&program).unwrap());
        assert_eq!(actual, expected, "VM and interpreter disagree on {input:?}");
        actual
    }

    fn error(input: &str) -> String {
        agree(input).unwrap_err().to_string()
    }

    fn compile_error(input: &str) -> String {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        compile(&program).unwrap_err().to_string()
    }

    #[test]
    fn arithmetic() {
        assert_eq!(agree("1 + 2 * 3"), Ok(Value::Int(7)));
        assert_eq!(agree("-(4 - 10) / 2"), Ok(Value::Int(3)));
        assert_eq!(agree("10 - 3 - 2"), Ok(Value::Int(5)));
        assert_eq!(agree("-9223372036854775808"), Ok(Value::Int(i64::MIN)));
    }

    #[test]
    fn comparisons() {
        assert_eq!(agree("1 + 1 == 2"), Ok(Value::Bool(true)));
        assert_eq!(agree("3 <= 2"), Ok(Value::Bool(false)));
        assert_eq!(agree("true != false"), Ok(Value::Bool(true)));
    }

    #[test]
    fn logical_operators() {
        assert_eq!(agree("!true"), Ok(Value::Bool(false)));
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
            assert_eq!(agree(input), Ok(Value::Bool(expected)), "{input}");
        }
    }

    #[test]
    fn logical_operators_short_circuit() {
        assert_eq!(agree("false && 1 / 0 == 0"), Ok(Value::Bool(false)));
        assert_eq!(agree("true || 1 / 0 == 0"), Ok(Value::Bool(true)));
    }

    #[test]
    fn if_else() {
        assert_eq!(agree("if 1 < 2 { 10 } else { 20 }"), Ok(Value::Int(10)));
        assert_eq!(agree("if 1 > 2 { 10 } else { 20 }"), Ok(Value::Int(20)));
        assert_eq!(
            agree("if false { 1 } else if false { 2 } else { 3 }"),
            Ok(Value::Int(3))
        );
        assert_eq!(agree("if false { 1 }"), Ok(Value::Unit));
    }

    #[test]
    fn blocks_and_statements() {
        assert_eq!(agree("{ 1; 2; 3 }"), Ok(Value::Int(3)));
        assert_eq!(agree("{ 1 + 2; }"), Ok(Value::Unit));
        assert_eq!(agree("{}"), Ok(Value::Unit));
        assert_eq!(agree("1; 2;"), Ok(Value::Unit));
        assert_eq!(agree(""), Ok(Value::Unit));
        assert_eq!(agree("if true { 1 } 2"), Ok(Value::Int(2)));
    }

    #[test]
    fn global_variables() {
        assert_eq!(agree("let x = 2; let y = x * 3; y + 1"), Ok(Value::Int(7)));
        assert_eq!(agree("let x = 1; x = x + 10; x"), Ok(Value::Int(11)));
        assert_eq!(agree("let x = 1; let x = x + 1; x"), Ok(Value::Int(2)));
    }

    #[test]
    fn local_variables() {
        assert_eq!(agree("{ let a = 1; let b = 2; a + b }"), Ok(Value::Int(3)));
        assert_eq!(
            agree("{ let a = 1; { let b = 2; a + b } }"),
            Ok(Value::Int(3))
        );
        assert_eq!(agree("let x = 1; { let x = x + 1; x }"), Ok(Value::Int(2)));
        assert_eq!(
            agree("let x = 1; { let x = 2; x = 3; } x"),
            Ok(Value::Int(1))
        );
        assert_eq!(
            agree("let x = 1; { let y = 2; x = x + y; } x"),
            Ok(Value::Int(3))
        );
    }

    #[test]
    fn locals_below_temporaries() {
        assert_eq!(
            agree("1 + { let a = 2; let b = 3; a * b }"),
            Ok(Value::Int(7))
        );
        assert_eq!(
            agree("let x = 5; if x > 3 { let y = x * 2; y } else { 0 }"),
            Ok(Value::Int(10))
        );
    }

    #[test]
    fn locals_dropped_after_block() {
        assert_eq!(
            agree("{ let a = 1; } { let b = 2; } { let c = 3; c }"),
            Ok(Value::Int(3))
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
    }

    #[test]
    fn undefined_variable_in_unreached_code_is_an_error() {
        // The interpreter only fails when it reaches `y`; the compiler checks every name.
        assert_eq!(
            compile_error("if false { y }"),
            "1:12: undefined variable: y"
        );
    }
}
