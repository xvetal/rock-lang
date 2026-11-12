mod ast;
mod interp;
mod lexer;
mod parser;

fn main() {
    match run("1 + 2 * 3") {
        Ok(value) => println!("{value}"),
        Err(message) => eprintln!("error: {message}"),
    }
}

fn run(source: &str) -> Result<i64, String> {
    let tokens = lexer::tokenize(source)?;
    let expr = parser::Parser::new(tokens).parse()?;
    interp::eval(&expr)
}
