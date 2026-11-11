mod ast;
mod interp;
mod lexer;
mod parser;

fn main() {
    let tokens = lexer::tokenize("1 + 2 * 3").unwrap();
    let expr = parser::Parser::new(tokens).parse().unwrap();
    println!("{expr:#?}");
    println!("= {:?}", interp::eval(&expr));
}
