#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Int(i64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        let token = match c {
            _ if c.is_whitespace() => continue,
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '0'..='9' => {
                let mut value = i64::from(c.to_digit(10).expect("c is an ASCII digit"));
                while let Some(&d) = chars.peek() {
                    let Some(digit) = d.to_digit(10) else { break };
                    chars.next();
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(i64::from(digit)))
                        .ok_or("integer literal is too large")?;
                }
                Token::Int(value)
            }
            _ => return Err(format!("unexpected character: '{c}'")),
        };

        tokens.push(token);
    }

    Ok(tokens)
}
