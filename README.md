# Rock

A toy programming language written in Rust, built for learning.

## Status

Version 0.2.0: an integer calculator with a command-line interface.

- Integer literals, `+ - * /`, unary minus, parentheses
- Operator precedence and left associativity
- Errors for division by zero and integer overflow
- Interactive REPL and running expressions from a file

## Goals

- Learn how lexers, parsers, and interpreters work
- Build a bytecode virtual machine
- Eventually generate native code

## Example

Planned syntax (subject to change):

```rock
let x = 1 + 2 * 3;
print(x);
```

## Usage

Install the `rock` binary from the repository root:

```sh
cargo install --path .
```

Start the interactive REPL (exit with Ctrl+D):

```sh
rock
> 1 + 2 * 3
7
```

Run an expression from a file:

```sh
rock run examples/calc.rk
```
