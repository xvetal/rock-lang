use std::collections::HashMap;
use std::io::Write;

use crate::ast::{BinaryOp, Expr, LogicalOp, Program, Stmt, UnaryOp};
use crate::value::Value;

pub struct Interpreter<W: Write> {
    vars: HashMap<String, Value>,
    out: W,
}

impl<W: Write> Interpreter<W> {
    pub fn new(out: W) -> Self {
        Self {
            vars: HashMap::new(),
            out,
        }
    }

    pub fn run(&mut self, program: &Program) -> Result<Value, String> {
        for stmt in &program.stmts {
            match stmt {
                Stmt::Let { name, value } => {
                    let value = self.eval(value)?;
                    self.vars.insert(name.clone(), value);
                }
                Stmt::Expr(expr) => {
                    self.eval(expr)?;
                }
            }
        }
        match &program.expr {
            Some(expr) => self.eval(expr),
            None => Ok(Value::Unit),
        }
    }

    fn eval(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Var(name) => self
                .vars
                .get(name)
                .copied()
                .ok_or_else(|| format!("undefined variable: {name}")),
            Expr::Call { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.eval(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call(name, &args)
            }
            Expr::Binary { op, left, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                binary(op, l, r)
            }
            Expr::Unary { op, operand } => {
                let value = self.eval(operand)?;
                match op {
                    UnaryOp::Neg => checked(int(value)?.checked_neg()),
                    UnaryOp::Not => Ok(Value::Bool(!boolean(value)?)),
                }
            }
            Expr::Logical { op, left, right } => {
                let l = boolean(self.eval(left)?)?;
                match (op, l) {
                    (LogicalOp::And, false) => Ok(Value::Bool(false)),
                    (LogicalOp::Or, true) => Ok(Value::Bool(true)),
                    _ => Ok(Value::Bool(boolean(self.eval(right)?)?)),
                }
            }
        }
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "print" => {
                let [value] = args else {
                    return Err(format!("print expects 1 argument, got {}", args.len()));
                };
                writeln!(self.out, "{value}").map_err(|e| format!("cannot write output: {e}"))?;
                Ok(Value::Unit)
            }
            _ => Err(format!("undefined function: {name}")),
        }
    }
}

fn binary(op: &BinaryOp, l: Value, r: Value) -> Result<Value, String> {
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
                return Err("division by zero".to_string());
            }
            checked(l.checked_div(r))
        }
    }
}

fn equal(l: Value, r: Value) -> Result<bool, String> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::Unit, Value::Unit) => Ok(true),
        (l, r) => Err(format!("cannot compare {l} with {r}")),
    }
}

fn int(value: Value) -> Result<i64, String> {
    match value {
        Value::Int(n) => Ok(n),
        other => Err(format!("expected integer, got {other}")),
    }
}

fn boolean(value: Value) -> Result<bool, String> {
    match value {
        Value::Bool(b) => Ok(b),
        other => Err(format!("expected bool, got {other}")),
    }
}

fn checked(result: Option<i64>) -> Result<Value, String> {
    result.map(Value::Int).ok_or("integer overflow".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn exec(interp: &mut Interpreter<Vec<u8>>, input: &str) -> Result<Value, String> {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        interp.run(&program)
    }

    fn value(input: &str) -> Result<Value, String> {
        exec(&mut Interpreter::new(Vec::new()), input)
    }

    fn run(input: &str) -> Result<i64, String> {
        match exec(&mut Interpreter::new(Vec::new()), input)? {
            Value::Int(n) => Ok(n),
            other => panic!("expected integer, got {other}"),
        }
    }

    fn output(input: &str) -> Result<String, String> {
        let mut interp = Interpreter::new(Vec::new());
        exec(&mut interp, input)?;
        Ok(String::from_utf8(interp.out).unwrap())
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
            Err("division by zero".to_string())
        );
        assert_eq!(String::from_utf8(interp.out).unwrap(), "1\n");
    }

    #[test]
    fn undefined_variable() {
        assert_eq!(run("x + 1"), Err("undefined variable: x".to_string()));
    }

    #[test]
    fn use_before_definition() {
        assert_eq!(
            run("let y = x; let x = 1; y"),
            Err("undefined variable: x".to_string())
        );
    }

    #[test]
    fn expression_statement_error() {
        assert_eq!(run("1 / 0; 2"), Err("division by zero".to_string()));
    }

    #[test]
    fn undefined_function() {
        assert_eq!(run("foo(1)"), Err("undefined function: foo".to_string()));
    }

    #[test]
    fn print_wrong_argument_count() {
        assert_eq!(
            run("print(1, 2)"),
            Err("print expects 1 argument, got 2".to_string())
        );
    }

    #[test]
    fn unit_in_arithmetic() {
        assert_eq!(
            run("print(1) + 1"),
            Err("expected integer, got ()".to_string())
        );
    }

    #[test]
    fn bool_in_arithmetic() {
        assert_eq!(
            run("true + 1"),
            Err("expected integer, got true".to_string())
        );
    }

    #[test]
    fn negation_of_bool() {
        assert_eq!(
            run("-false"),
            Err("expected integer, got false".to_string())
        );
    }

    #[test]
    fn compare_int_with_bool() {
        assert_eq!(
            value("1 == true"),
            Err("cannot compare 1 with true".to_string())
        );
    }

    #[test]
    fn ordering_of_bools() {
        assert_eq!(
            value("false < true"),
            Err("expected integer, got false".to_string())
        );
    }

    #[test]
    fn not_of_int() {
        assert_eq!(value("!1"), Err("expected bool, got 1".to_string()));
    }

    #[test]
    fn and_with_int() {
        assert_eq!(value("1 && true"), Err("expected bool, got 1".to_string()));
    }

    #[test]
    fn evaluated_right_operand_must_be_bool() {
        assert_eq!(
            value("true && print(1)"),
            Err("expected bool, got ()".to_string())
        );
    }

    #[test]
    fn division_by_zero() {
        assert_eq!(run("1 / 0"), Err("division by zero".to_string()));
    }

    #[test]
    fn computed_division_by_zero() {
        assert_eq!(run("1 / (2 - 2)"), Err("division by zero".to_string()));
    }

    #[test]
    fn addition_overflow() {
        assert_eq!(
            run("9223372036854775807 + 1"),
            Err("integer overflow".to_string())
        );
    }

    #[test]
    fn subtraction_overflow() {
        assert_eq!(
            run("0 - 9223372036854775807 - 2"),
            Err("integer overflow".to_string())
        );
    }

    #[test]
    fn multiplication_overflow() {
        assert_eq!(
            run("9223372036854775807 * 2"),
            Err("integer overflow".to_string())
        );
    }

    #[test]
    fn negation_overflow() {
        assert_eq!(
            run("-(-9223372036854775807 - 1)"),
            Err("integer overflow".to_string())
        );
    }

    #[test]
    fn negation_of_min_value_overflows() {
        assert_eq!(
            run("--9223372036854775808"),
            Err("integer overflow".to_string())
        );
    }

    #[test]
    fn min_value_divided_by_minus_one() {
        assert_eq!(
            run("-9223372036854775808 / -1"),
            Err("integer overflow".to_string())
        );
    }
}
