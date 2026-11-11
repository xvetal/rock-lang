mod ast;
mod lexer;
mod parser;

fn main() {
    let tokens = lexer::tokenize("1 + 2 * 3").unwrap();
    let expr = parser::Parser::new(tokens).parse();
    println!("{expr:#?}");
}
