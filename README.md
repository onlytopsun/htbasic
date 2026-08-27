# HTBasic Interpreter

A Rust implementation of the **HTBasic / Rocky Mountain BASIC** programming language — a dialect of BASIC historically used by HP for test & measurement and instrument control.

**100 tests passing. 0 compiler warnings. ~7,500 lines of Rust.**

## Quick Start

```bash
# Build
cargo build --release

# Start interactive REPL
cargo run

# Run a program file
cargo run -- examples/demo.bas

# Run with experimental bytecode VM
cargo run -- --bytecode program.bas

# Pipe commands to REPL (batch mode)
echo 'PRINT "Hello World"' | cargo run
```

## TransEra Container Converter

Convert HTBwin95 binary program containers (tokenized `.prg` files) to ASCII
BASIC source this interpreter can run:

```bash
cargo run -- -c "C:\Program Files (x86)\HTBwin95\examples\print.prg"   # one file
cargo run -- -C "C:\Program Files (x86)\HTBwin95\examples" -O converted --report   # batch
cargo run -- --check-dir converted                                      # parse-check results
```

See [CONVERTER.md](CONVERTER.md) for the full guide (all flags, output details,
DLL handling, library API, caveats).

## REPL Commands

The interactive REPL supports both immediate execution and program editing:

```
HTBasic> PRINT "Hello, World!"
Hello, World!

HTBasic> X = 42
HTBasic> PRINT X * 2
84

HTBasic> 10 PRINT "Line 10"
  Line 10 stored
HTBasic> 20 PRINT "Line 20"
  Line 20 stored
HTBasic> LIST
10 PRINT "Line 10"
20 PRINT "Line 20"
HTBasic> RUN
Line 10
Line 20
```

### All REPL Commands

| Command | Description |
|---------|-------------|
| `RUN` | Execute the program in memory |
| `LIST [range]` | List program (e.g., `LIST 100-200`) |
| `SAVE [file]` | Save to `.BAS` (ASCII) or `.HTB` (binary) |
| `LOAD [file]` | Load from `.BAS` file |
| `GET [file]` | Load from `.HTB` binary file |
| `MERGE [file]` | Merge lines from file into current program |
| `EDIT <line>` | Edit a specific line |
| `AUTO [start]` | Auto-number new lines |
| `DEL <range>` | Delete range of lines |
| `REN [start]` | Renumber lines starting at `start` (step 10) |
| `SCRATCH` | Clear program and variables |
| `HELP` | Show available commands |
| `EXIT` / `QUIT` | Exit the interpreter |

Type a line number to add/replace a program line. Type just a line number to delete it. Type a statement without a line number to execute it immediately.

## Language Features

### Variables and Types

```basic
X = 42                ! Implicit LET
Name$ = "HTBasic"     ! String variable ($ suffix)
DIM A(10), B(3,4)     ! Array declaration
INTEGER I, J          ! Type declaration
OPTION BASE 1         ! Set array lower bound to 1
COM /Block/ REAL X    ! COMMON block
```

### Control Flow

```basic
IF X > 5 THEN PRINT "Big" ELSE PRINT "Small"

FOR I = 1 TO 10 STEP 2
    PRINT I
NEXT I

WHILE X > 0
    X = X - 1
END WHILE

LOOP
    EXIT IF X > 100
    X = X + 1
END LOOP

REPEAT
    X = X + 1
UNTIL X > 10

SELECT X
CASE 1: PRINT "One"
CASE 2 TO 5: PRINT "Two to Five"
CASE ELSE: PRINT "Other"
END SELECT

GOTO Label
GOSUB Subroutine
ON I GOTO L1, L2, L3
ON I GOSUB S1, S2, S3
```

### Subprograms and Functions

```basic
CALL SayHello("World")
END

SUB SayHello(Name$)
    PRINT "Hello " & Name$
SUBEND

DEF FNSquare(X)
    RETURN X * X
FNEND

PRINT FNSquare(5)   ! Outputs: 25
```

### Matrix Operations

```basic
DIM A(3,3), B(3,3), C(3,3)
MAT C = A + B        ! Element-wise addition
MAT B = TRN(A)       ! Transpose
MAT A = IDN          ! Identity matrix
MAT A = ZER          ! Zero matrix
MAT A = CON          ! Ones matrix
MAT A = INV(B)       ! Inverse
```

### Built-in Functions (80+)

| Category | Functions |
|----------|-----------|
| **Math** | `ABS`, `SQR`, `EXP`, `LOG`, `LOG10`, `INT`, `FRACT`, `CEIL`, `FLOOR`, `ROUND`, `TRUNCATE`, `SGN`, `MAX`, `MIN`, `RND`, `PI` |
| **Trig** | `SIN`, `COS`, `TAN`, `ASIN`, `ACOS`, `ATN` |
| **Hyperbolic** | `SINH`, `COSH`, `TANH` |
| **Strings** | `LEN`, `UPC$`, `LWC$`, `TRIM$`, `LTRIM$`, `RTRIM$`, `REV$`, `RPT$`, `CHR$`, `STR$`, `VAL`, `NUM`, `POS`, `INSTR`, `SPACE$`, `STRING$` |
| **Date/Time** | `DATE$`, `TIME$`, `TIMEDATE` |
| **Bit** | `BIT`, `BINAND`, `BINOR`, `BINXOR`, `BINNOT`, `SHL`, `SHR` |
| **System** | `SYSTEM$`, `ENVIRON$` |

### Graphics (35 commands)

```basic
GINIT
WINDOW 0, 100, 0, 100
MOVE 10, 10
DRAW 90, 90
PEN 2
COLOR "Red"
LABEL "Hello Graphics"
RECTANGLE 20, 20, 80, 80
AXES 10, 10, 0, 0
GSTORE "output.png"     ! Save PNG
```

Full coordinate pipeline: world coords → window → viewport → clip → pixels. Supports Bresenham line drawing, polygon scanline fill, 5×7 bitmap font, 16-color pen palette, dashed line patterns, and PNG output.

### Event Handling

```basic
ON KEY 1 GOTO KeyHandler
ON END GOTO Cleanup
ON CYCLE 5 GOSUB PollTimer
ON TIMEOUT @Device, 15 GOTO Retry
ENABLE
DISABLE
OFF KEY
```

### File I/O

```basic
ASSIGN @Dev TO "data.txt"
OUTPUT @Dev; 42, "Hello"
ENTER @Dev; X
CREATE "newfile.txt"
PURGE "oldfile.txt"
CAT "*"
MASS STORAGE IS "C:\"
```

### Program Operations

```basic
SAVE "program.bas"     ! Save as ASCII
SAVE "program.htb"     ! Save as binary
GET "program.htb"      ! Load binary
LOAD "program.bas"     ! Load ASCII
MERGE "library.bas"    ! Merge lines
CHAIN "next.bas"       ! Run another program
```

## Architecture

```
Source .BAS → Lexer → Parser → AST → Tree-walking Interpreter → Output
                                    ↘ Bytecode VM (--bytecode flag)
```

### Project Structure

```
htbasic/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library root
│   ├── error.rs             # Error types (thiserror + miette)
│   ├── program.rs           # Program buffer for REPL
│   ├── repl.rs              # Interactive REPL + batch mode
│   ├── lexer/
│   │   ├── token.rs         # 60+ token types
│   │   └── lexer.rs         # Line-oriented tokenizer
│   ├── parser/
│   │   ├── ast.rs           # AST node types (statements, expressions)
│   │   ├── parser.rs        # Recursive descent + Pratt parser
│   │   └── precedence.rs    # Operator precedence table
│   ├── analyzer/
│   │   ├── scope.rs         # Symbol table (stub)
│   │   └── resolver.rs      # Name resolution (stub)
│   └── runtime/
│       ├── value.rs         # Runtime value types
│       ├── builtins.rs      # 80+ built-in functions
│       ├── interpreter.rs   # Tree-walking interpreter
│       ├── bytecode.rs      # Bytecode compiler + stack VM
│       ├── graphics.rs      # 2D graphics pipeline
│       └── io.rs            # File/device I/O registry
├── tests/
│   └── integration_tests.rs # 100 integration tests
├── examples/
│   └── demo.bas             # Demo program
└── Cargo.toml
```

### Key Design Decisions

- **Hand-written recursive descent parser** with Pratt expression parsing — gives full control over HTBasic's unusual operator precedence (unary minus below exponentiation)
- **Flat instruction vector** for GOTO/GOSUB support — tree-walking interpreter uses a statement vector with program counter rather than recursive evaluation
- **Separate bytecode VM** — stack-based with 35 opcodes, constant pool, gosub/call stacks, forward-jump patching
- **Line-oriented lexer** — handles HTBasic's line numbers, labels, multi-statement lines (`:`), continuation (`&`)

## Running Tests

```bash
# Run all 100 tests
cargo test

# Run specific test
cargo test test_for_loop

# Run bytecode VM tests
cargo test test_vm
```

## Requirements

- Rust 1.70+ (2021 edition)
- Dependencies: `thiserror`, `miette`, `image` (PNG), `rustyline` (REPL)

## License

MIT

## HTBasic Language Reference

HTBasic is a modern implementation of Rocky Mountain BASIC (RMB), originally developed by HP in the 1970s for instrument control and test automation. This interpreter aims for compatibility with TransEra HTBasic for Windows.

Key references:
- [TransEra HTBasic Help](https://transera.com/help/)
- [HP BASIC/WS Manuals](https://www.eserviceinfo.com/downloadsm/81581/HP_98613-90052%20Basic%205.0%20Language%20Reference%20Vol%202%20Aug88.html)
