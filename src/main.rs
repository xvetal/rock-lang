mod ast;
mod chunk;
mod compiler;
// Kept only as the reference the VM tests compare against.
#[cfg_attr(not(test), allow(dead_code))]
mod interp;
mod lexer;
mod parser;
mod value;
mod vm;

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::panic;
use std::process::ExitCode;
use std::thread;

use compiler::Scope;
use interp::STACK_SIZE;
use value::Value;
use vm::Vm;

fn main() -> ExitCode {
    thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(cli)
        .expect("cli thread should start")
        .join()
        .unwrap_or_else(|payload| panic::resume_unwind(payload))
}

fn cli() -> ExitCode {
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
    let mut scope = Scope::default();
    let mut vm = Vm::new(io::stdout());
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

        report(run(&mut vm, &mut scope, &line));
    }
}

fn run_file(path: &str) -> Result<Value, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("cannot read '{path}': {e}"))?;
    run(&mut Vm::new(io::stdout()), &mut Scope::default(), &source)
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

fn run(vm: &mut Vm<io::Stdout>, scope: &mut Scope, source: &str) -> Result<Value, String> {
    let tokens = lexer::tokenize(source)?;
    let program = parser::Parser::new(tokens)
        .parse()
        .map_err(|e| e.to_string())?;
    let bytecode = compiler::compile(&program, scope).map_err(|e| e.to_string())?;
    let value = vm.run(&bytecode).map_err(|e| e.to_string())?;
    *scope = bytecode.scope;
    Ok(value)
}
