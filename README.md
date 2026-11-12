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

```rock
let x = 1 + 2 * 3;
print(x);
let y = x * x;
y - x
```

Running it prints `7` from `print`, then `42`, the value of the final expression.

## Usage

Install the `rock` binary from the repository root:

```sh
cargo install --path .
```

Start the interactive REPL (exit with Ctrl+D):

```sh
rock
> let x = 1 + 2 * 3;
> x * 2
14
```

Run a program from a file:

```sh
rock run examples/print.rk
```
