use crate::ast::{BinaryOp, Expr};

pub fn eval(expr: &Expr) -> Result<i64, String> {
    match expr {
        Expr::Int(n) => Ok(*n),
        Expr::Binary { op, left, right } => {
            let l = eval(left)?;
            let r = eval(right)?;
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
            result.ok_or("integer overflow".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn run(input: &str) -> Result<i64, String> {
        let tokens = tokenize(input).unwrap();
        let expr = Parser::new(tokens).parse().unwrap();
        eval(&expr)
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
}
