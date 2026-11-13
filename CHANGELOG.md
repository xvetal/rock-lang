# Changelog

## [Unreleased]

### Added
- Boolean literals `true` and `false`
- Comparison operators `==`, `!=`, `<`, `<=`, `>`, `>=`; comparisons cannot be chained

### Changed
- Error messages show tokens as written: `unexpected ')'` instead of `unexpected token: RParen`
- Parse errors say what was expected and what was found: `expected ')', found end of input`
- Errors report line and column: `1:4: expected expression, found end of input`

## [0.3.0] - 2026-11-13

### Added
- Variables: `let x = 1 + 2;` binds a name, `x` reads it
- A program is a sequence of statements optionally followed by an expression, whose value is printed
- Shadowing: `let x = x + 1;` sees the previous `x`
- The REPL keeps variables between lines
- Error for undefined variables
- Expression statements: `expr;` evaluates and discards the value
- Function calls: `f(a, b)`
- Built-in `print(x)` writes a value followed by a newline
- Unit value `()` returned by `print`

## [0.2.0] - 2026-11-12

### Added
- Interactive REPL: run `rock` without arguments
- `rock run <file>` evaluates an expression from a file
- Exit codes: 0 on success, 1 on errors, 2 on invalid usage
- Unary minus: `-5`, `-(1 + 2)`, `2 * -3`
- Integer literal `-9223372036854775808` (`i64::MIN`)

## [0.1.0] - 2026-11-12

### Added
- Lexer for integer literals, `+ - * /` and parentheses
- Recursive descent parser with operator precedence and left associativity
- Tree-walking interpreter with division-by-zero and overflow errors
