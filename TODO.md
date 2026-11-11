# TODO

## Lexer
- [ ] Report line and column in errors (add `Span` to tokens)

## Parser
- [ ] Support `i64::MIN` literal (needs unary minus)
- [ ] Human-readable errors: implement `Display` for `Token`
      (`unexpected ')'` instead of `unexpected token: RParen`)
- [ ] Replace `String` errors with `enum ParseError`
- [ ] `"()"` should report `expected expression`
- [ ] Limit nesting depth to avoid stack overflow on deeply nested input
- [ ] Switch to a Pratt parser when more precedence levels appear
