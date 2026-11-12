# Rock

A toy programming language written in Rust, built for learning.

## Status

Version 0.1.0: an integer calculator.

- Integer literals, `+ - * /`, parentheses
- Operator precedence and left associativity
- Errors for division by zero and integer overflow

The input expression is hardcoded in `src/main.rs` for now; a CLI is next.

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

Planned CLI (not implemented yet):

```sh
rock run hello.rk
```
