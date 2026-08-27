# TransEra Container Converter Guide

The converter decodes TransEra **HTBwin95 binary program containers** — tokenized
`.prg` files (and the `.bas` ASCII containers HTBasic itself saves) — into plain
ASCII BASIC source that this interpreter can parse and run.

The container format has no public specification; it was reverse-engineered from
real files. Decoding is deliberately tolerant: unknown opcodes become
placeholders or comments and never abort a conversion.

## Container types

| Magic (first 2 bytes) | Kind        | Handling                                    |
|-----------------------|-------------|---------------------------------------------|
| `86 84`               | Tokenized   | Full decode (`.prg` program images)         |
| `88 84`               | ASCII       | Line records are verbatim source; decoded   |
| `87 84`               | Other       | Rejected — keyboard/install files, not programs |

## Commands

All converter commands run through the main binary:

```bash
cargo run -- <flags>        # during development
htbasic <flags>             # from a release build
```

### Convert one file

```bash
cargo run -- -c "C:\Program Files (x86)\HTBwin95\examples\print.prg"
# -> writes print.bas next to the input
```

- `-o, --out <path>` — output path instead of the default `<stem>.bas`.
- Refuses to overwrite the input container itself; use `--out` to write
  elsewhere.
- Decode warnings print to stderr as `warning @0x…` lines.

### Batch-convert a directory

```bash
cargo run -- -C "C:\Program Files (x86)\HTBwin95\examples" -O converted --report
```

- Non-recursive. Every file in the directory is tried; non-containers are
  skipped silently (counted in the summary).
- Output: `<stem>.bas` in the output directory (default `./converted`, which is
  gitignored).
- Prints a summary: `Converted N file(s), skipped N non-container(s), failed N, warnings N`.
- `--report` adds an unknown-opcode histogram and the top 30 warning messages —
  use it to decide which opcodes to add to the token tables next.
- Exit code is non-zero if any file failed (or, with `--strict`, if any file
  produced warnings).

> **Same-stem collision**: if a directory contains both `print.prg` and
> `print.bas`, both write `converted\print.bas` and the later one wins. Convert
> `.prg`-only directories, or point different batches at different output
> directories.

### Structured token dump

```bash
cargo run -- -d "C:\Program Files (x86)\HTBwin95\examples\print.prg"
```

Decodes only — prints the container summary, every section (type, dialect
marker, geometry, name table), and every line's token stream with line number,
indent field, and flag byte. Useful for diagnosing decode issues and for
reverse-engineering the format further.

### Parse-check

```bash
cargo run -- --check "C:\Program Files (x86)\HTBwin95\examples\print.prg"
cargo run -- --check-dir converted
```

`--check` converts one container and runs the result through this project's own
parser (never executes it); prints `OK` / `FAIL`, exit 1 on failure.

`--check-dir` parse-checks every `.bas` in a directory and prints a summary plus
the failing files with line numbers. Always exits 0 — it is an aggregate report,
not a per-file gate.

### `--strict`

Treat decode warnings as errors: `--convert`/`--convert-dir` exit non-zero when
any file produced warnings.

## What the emitted source looks like

- Line numbers and indentation preserved exactly (from the record's X field).
- Comments as `! text` (bare comment lines get a single space).
- Multiple statements joined with ` : `.
- Labels as `Finish:`; SUB sections as `SUB name` … `SUBEND`.
- TransEra save conventions: `LET` omitted, fractional reals without leading
  zero (`WAIT .1`, not `WAIT 0.1`).
- **DLL statements** are emitted as `! DLL …` comments by default, because this
  interpreter has no Windows-DLL support. Real `DLL LOAD` / `DLL GET … AS …` /
  `DLL UNLOAD ALL` statements are only produced via the library API (see below)
  — note such programs still will not run fully here either way.
- **Unknown opcodes**: mid-expression bytes become `UhXX` placeholders; a whole
  undecodable statement becomes a `! U <hex>` comment. Both are reported in
  warnings and in `--report`.

## Typical workflow

```bash
# 1. Batch-convert all TransEra examples
cargo run -- -C "C:\Program Files (x86)\HTBwin95\examples" -O converted --report

# 2. Parse-check the results; fix the worst offenders by adding
#    opcodes to src/converter/tokens.rs, then repeat
cargo run -- --check-dir converted

# 3. Spot-run converted programs through the interpreter
cargo run -- converted\print.bas
```

## Library API

Decode and emission are separate, so callers can inspect or transform the
structured form between them:

```rust
use htbasic::converter::{decode, emit_source, ConvertOptions};

let parsed = decode(&bytes)?; // ParsedFile { kind, variant, sections, warnings, unknown_opcodes }

let source = emit_source(
    &parsed,
    &ConvertOptions {
        comment_out_dll: false, // render real DLL statements instead of ! DLL comments
        ..ConvertOptions::default()
    },
);
```

`decode` returns `ConvertError` variants (`NotAContainer`, `UnsupportedContainer`,
`TooShort`, `BadSectionHeader`) — byte-offset oriented, independent of the
interpreter's `HtBasicError`/miette machinery.

## Notes

- **RTK wrapper**: on this machine `cargo` goes through the RTK proxy, which
  filters output — use `rtk proxy cargo run -- …` to bypass it.
- **Warnings go to stderr** — keep stderr visible (don't `2>$null`) or you lose
  the `warning @0x…` diagnostics.
- **Ground-truth tests**: `cargo test --features htbwin-fixtures -- --ignored`
  compares decode output against real TransEra `.prg`/`.bas` pairs (print and
  HTBClipboard). Dev-machine only, never run in CI.
- **Copyright**: converted output is derived from copyrighted TransEra
  originals — `/converted/` is gitignored and TransEra files are never committed.

## Exit codes

| Code | Meaning |
|------|---------|
| 0    | Success |
| 1    | Decode/parse/write failure (or `--strict` warnings) |
| 2    | Usage error (unknown flag, missing value, no mode) |
