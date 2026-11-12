use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, Program, Stmt, UnaryOp};
use crate::value::Value;

#[derive(Default)]
pub struct Interpreter {
    vars: HashMap<String, Value>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn run(&mut self, program: &Program) -> Result<Value, String> {
        for stmt in &program.stmts {
            match stmt {
                Stmt::Let { name, value } => {
                    let value = self.eval(value)?;
                    self.vars.insert(name.clone(), value);
                }
            }
        }
        match &program.expr {
            Some(expr) => self.eval(expr),
            None => Ok(Value::Unit),
        }
    }

    fn eval(&self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Var(name) => self
                .vars
                .get(name)
                .copied()
                .ok_or_else(|| format!("undefined variable: {name}")),
            Expr::Binary { op, left, right } => {
                let l = self.eval_int(left)?;
                let r = self.eval_int(right)?;
                let result = match op {
                    BinaryOp::Add => l.checked_add(r),
                    BinaryOp::Sub => l.checked_sub(r),
                    BinaryOp::Mul => l.checked_mul(r),
                    BinaryOp::Div => {
                        if r == 0 {
                            return Err("division by zero".to_string());
                        }
                        l.checked_div(r)
                    }
                };
                result.map(Value::Int).ok_or("integer overflow".to_string())
            }
            Expr::Unary { op, operand } => {
                let value = self.eval_int(operand)?;
                let result = match op {
                    UnaryOp::Neg => value.checked_neg(),
                };
                result.map(Value::Int).ok_or("integer overflow".to_string())
            }
        }
    }

    fn eval_int(&self, expr: &Expr) -> Result<i64, String> {
        match self.eval(expr)? {
            Value::Int(n) => Ok(n),
            other => Err(format!("expected integer, got {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn exec(interp: &mut Interpreter, input: &str) -> Result<Value, String> {
        let program = Parser::new(tokenize(input).unwrap()).parse().unwrap();
        interp.run(&program)
    }

    fn run(input: &str) -> Result<i64, String> {
        match exec(&mut Interpreter::new(), input)? {
            Value::Int(n) => Ok(n),
            other => panic!("expected integer, got {other}"),
        }
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
    fn let_bindings() {
        assert_eq!(run("let x = 1 + 2; let y = x * 3; y - 1"), Ok(8));
    }

    #[test]
    fn shadowing() {
        assert_eq!(run("let x = 1; let x = x + 1; x"), Ok(2));
    }

    #[test]
    fn statements_only() {
        assert_eq!(exec(&mut Interpreter::new(), "let x = 1;"), Ok(Value::Unit));
    }

    #[test]
    fn variables_persist_between_runs() {
        let mut interp = Interpreter::new();
        assert_eq!(exec(&mut interp, "let x = 2;"), Ok(Value::Unit));
        assert_eq!(exec(&mut interp, "x * 3"), Ok(Value::Int(6)));
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
