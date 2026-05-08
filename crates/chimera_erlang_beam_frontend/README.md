# Chimera-Erlang-BEAM Frontend (OCaml)

OCaml compiler frontend for Chimera-Erlang-BEAM.

## Components

- **lexer.mll** - OCamllex tokenizer for Erlang syntax
- **parser.mly** - Menhir parser producing Erlang AST
- **core_ir.ml** - Core Erlang IR and lowering

## Building

Requires OCaml 5.0+ and Dune:

```bash
dune build
dune test
```

## Status

Stubs only - full implementation pending.