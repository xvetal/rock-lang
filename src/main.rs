mod ast;
mod interp;
mod lexer;
mod parser;
mod value;

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use interp::Interpreter;
use value::Value;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.as_slice() {
        [_] => {
            repl();
            ExitCode::SUCCESS
        }
        [_, cmd, path] if cmd == "run" => report(run_file(path)),
        _ => {
            eprintln!("usage: rock [run <file>]");
            ExitCode::from(2)
        }
    }
}

fn repl() {
    let stdin = io::stdin();
    let mut line = String::new();
    let mut interp = Interpreter::new();
    loop {
        print!("> ");
        io::stdout().flush().expect("stdout should be writable");

        line.clear();
        let bytes = stdin
            .lock()
            .read_line(&mut line)
            .expect("stdin should be readable");
        if bytes == 0 {
            println!();
            break;
        }

        if line.trim().is_empty() {
            continue;
        }

        report(run(&mut interp, &line));
    }
}

fn run_file(path: &str) -> Result<Value, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    run(&mut Interpreter::new(), &source)
}

fn report(result: Result<Value, String>) -> ExitCode {
    match result {
        Ok(Value::Unit) => ExitCode::SUCCESS,
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(interp: &mut Interpreter, source: &str) -> Result<Value, String> {
    let tokens = lexer::tokenize(source)?;
    let program = parser::Parser::new(tokens).parse()?;
    interp.run(&program)
}
