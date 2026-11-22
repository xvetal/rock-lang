# Rock

A toy programming language written in Rust, built for learning.

## Status

Version 0.14.0: programs are statically typed and type-checked before they run, then executed on a bytecode virtual machine; all syntax and type errors are reported at once, together with warnings for unused variables and unneeded `mut`.

- Integer, boolean and string literals, `+ - * /`, unary minus, parentheses
- Strings with the escapes `\n`, `\t`, `\\` and `\"`, concatenation with `+`, and comparison by character codes
- Comparisons `== != < <= > >=` and logical `! && ||` with short-circuit
- Operator precedence and left associativity
- Variables with `let`, immutable unless declared with `let mut`, with an optional type such as `let a: [int] = [];`, including shadowing, and assignment `x = value;`
- Line comments `// ...`
- Blocks with lexical scope; `if`/`else` and blocks are expressions
- `while` loops, `for i in a..b` loops over integers and `for x in a` loops over arrays, with `break` and `continue`
- Functions with `fn` and `return`, including recursion; parameters and return values have types, e.g. `fn add(a: int, b: int) -> int`; parameters can be `mut`; functions are values, can be called before their declaration, and see only their parameters and locals
- Arrays such as `[1, 2, 3]` with indexing `a[i]`, assignment `a[i] = v` and appending with `push(a, v)`; arrays are values, so a change never reaches a copy
- Slices `s[a..b]`, `s[a..]` and `s[..b]` of strings, by bytes, and of arrays
- The built-in `print`, `len`, which returns the length of a string in bytes or of an array, `push`, and `bytes`, which returns the bytes of a string as `[int]`
- Static types `int`, `bool`, `str`, `()`, array types such as `[int]` and function types such as `fn(int) -> int`
- Errors with line and column: parse errors (including nesting deeper than 256 levels) and type errors (mismatched types, undefined names, wrong argument counts, assignment to immutable variables), all reported at once, and runtime errors (division by zero, integer overflow, stack overflow, index out of bounds, bad slice bounds)
- Warnings with line and column: unused variables and parameters (names starting with `_` are exempt) and `mut` that is never needed
- Interactive REPL, running programs from a file, and checking them without running with `rock check`

## Goals

- Learn how lexers, parsers, and interpreters work
- Build a bytecode virtual machine
- Eventually generate native code

## Example

```rock
let mut n = 5;
let mut acc = 1;
while n > 1 {
    acc = acc * n;
    n = n - 1;
}
print(acc); // 120
acc > 100
```

Running it prints `120` from `print`, then `true`, the value of the final expression.

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

Check a program for errors without running it:

```sh
rock check examples/print.rk
```
