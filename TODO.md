# TODO

## Lexer
- [ ] Report line and column in errors (add `Span` to tokens)

## Parser
- [ ] Replace `String` errors with `enum ParseError`
- [ ] Limit nesting depth to avoid stack overflow on deeply nested input
- [ ] Switch to a Pratt parser when more precedence levels appear
