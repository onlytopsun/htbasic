# CLAUDE.md — HTBasic Interpreter Project

## Build & Test

```bash
# Build
cargo build                    # Debug build
cargo build --release          # Release build

# Run
cargo run                      # Start REPL
cargo run -- file.bas          # Execute a .BAS file
cargo run -- --bytecode file.bas  # Use experimental bytecode VM

# Test
cargo test                     # Run all 100 tests
cargo test test_for_loop       # Run specific test by name filter
cargo test -- --test-threads=1 # Run serially (avoids binary lock on Windows)

# Check without building
cargo check                    # Fast compile-check
cargo check --tests            # Include test code

# Clean
cargo clean                    # Wipe target/ (use when test binary gets locked)
```

## Project Structure

```
src/
├── main.rs              # CLI: parses args, dispatches to REPL or file execution
├── lib.rs               # Library root — re-exports all modules
├── error.rs             # HtBasicError enum with Span tracking (thiserror + miette)
├── program.rs           # ProgramBuffer for REPL line-number editing
├── repl.rs              # REPL loop: rustyline interactive + batch stdin mode
├── lexer/
│   ├── token.rs         # TokenKind enum (60+ variants)
│   └── lexer.rs         # Line-oriented lexer with multi-word keyword matching
├── parser/
│   ├── ast.rs           # Stmt, Expr, GfxCmd enums + subprogram/fn types
│   ├── parser.rs        # Recursive descent + Pratt expression parser
│   └── precedence.rs    # Operator precedence table
├── analyzer/
│   ├── scope.rs         # SymbolTable stub (future semantic analysis)
│   └── resolver.rs      # Resolver stub
└── runtime/
    ├── value.rs         # Value enum (Real/Integer/String/Array/Null)
    ├── builtins.rs      # 80+ built-in functions + seeded RNG + date helpers
    ├── interpreter.rs   # Tree-walking interpreter with flat Instr vector
    ├── bytecode.rs      # Bytecode compiler + stack-based VM (experimental)
    ├── graphics.rs      # 2D graphics: coordinate pipeline, drawing, PNG output
    └── io.rs            # File/device I/O registry

tests/
└── integration_tests.rs # 100 tests: lexer, tree-walking, VM, graphics, SUB/FN
```

## Architecture

Two execution paths:
1. **Tree-walking interpreter** (default) — `Parser → Interpreter::new(program).run()`
2. **Bytecode VM** (`--bytecode` flag) — `Parser → Compiler.compile() → VM::new(chunk).run()`

The tree-walking interpreter uses a flat `Vec<Instr>` for correct GOTO/GOSUB support. Each statement is compiled into an `Instr::Stmt(...)` and executed via a program counter, not recursive descent.

## Key Patterns

- **Variable lookup**: `Scope` uses `HashMap<String, Value>` with optional parent chain. `get()` searches upward, `set()` writes to current scope only.
- **Error handling**: `HtBasicError` uses `thiserror` + `miette` with `Span` for source locations. Use `self.runtime_error(code, msg)` to raise errors with ON ERROR handler support.
- **CONFIGURE routing**: Many keywords (ASSIGN, OUTPUT, ENTER, STATUS, CONTROL, ON KEY, etc.) are parsed as `Stmt::Configure(key, value, span)` and dispatched in the interpreter's configure handler.
- **IF parsing**: Single-line IF (`IF x THEN stmt`) is checked by looking for Newline after THEN. Without Newline, it enters multi-line mode. Multi-line IF loops exit on `EndIf`, `Else`, `SubEnd`, `FnEnd`, or `End`.
- **DEF FN**: Function names get "FN" prepended (`parse_fn_def` adds prefix) because the lexer strips "FN" as part of the `DEF FN` compound keyword. Registry key = "FNSQUARE", call site = `FNSquare(5)`.

## Test Patterns

- `run_source("...")` — compiles and runs through tree-walking interpreter
- `run_bytecode("...")` — compiles and runs through bytecode VM
- Tests that hang (infinite loop) should use `#[ignore]` or be fixed
- Run tests with `--test-threads=1` on Windows to avoid binary lock errors

## Known Issues

1. **VM GOSUB** — pc-offset mismatch in bytecode VM's Gosub/Return handlers. Tree-walking interpreter works correctly.
2. **VM FOR loop** — fixed by pushing end/step back onto stack before jumping back. Verify with `test_vm_for_loop`.
3. **Test binary lock** — on Windows, `cargo test` may fail with LINK error 1104. Fix: `rm target/debug/deps/integration_tests-*.exe` then rebuild.
4. **rtk wrapper** — `cargo` commands go through RTK proxy which filters output. Use `rtk proxy <cmd>` to bypass.

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `thiserror` | 2 | Error derive macros |
| `miette` | 7 | Diagnostic error rendering |
| `image` | 0.25 | PNG read/write for GLOAD/GSTORE |
| `rustyline` | 15 | REPL line editing + history |
