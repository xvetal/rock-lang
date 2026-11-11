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
