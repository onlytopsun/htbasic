//! Converter CLI: `--convert`, `--convert-dir`, `--dump-tokens`, `--check`,
//! `--check-dir`, plus `--report`, `--strict`, and output options.

use super::{decode, emit_source, ConvertError, ConvertOptions, ParsedFile};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

enum Mode {
    Convert(PathBuf),
    ConvertDir(PathBuf),
    Dump(PathBuf),
    Check(PathBuf),
    CheckDir(PathBuf),
}

pub fn run(args: &[String]) -> ! {
    process::exit(run_inner(args));
}

fn run_inner(args: &[String]) -> i32 {
    let mut iter = args.iter().peekable();
    let mut mode: Option<Mode> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut report = false;
    let mut strict = false;

    while let Some(arg) = iter.next() {
        let mut set_mode = |m: Mode| {
            if mode.is_none() {
                mode = Some(m);
            }
        };
        let mut next_value = || {
            iter.next()
                .map(|v| PathBuf::from(v.as_str()))
                .unwrap_or_else(|| {
                    eprintln!("Error: flag '{arg}' needs a value");
                    process::exit(2);
                })
        };
        match arg.as_str() {
            "-c" | "--convert" => set_mode(Mode::Convert(next_value())),
            "-o" | "--out" => out_path = Some(next_value()),
            "-C" | "--convert-dir" => set_mode(Mode::ConvertDir(next_value())),
            "-O" | "--out-dir" => out_dir = Some(next_value()),
            "-d" | "--dump-tokens" => set_mode(Mode::Dump(next_value())),
            "--check" => set_mode(Mode::Check(next_value())),
            "--check-dir" => set_mode(Mode::CheckDir(next_value())),
            "--report" => report = true,
            "--strict" => strict = true,
            "--help" | "-h" => {
                print_help();
                return 0;
            },
            other => {
                eprintln!("Error: unknown converter flag '{other}'");
                print_help();
                return 2;
            },
        }
    }

    let Some(mode) = mode else {
        print_help();
        return 2;
    };
    let opts = ConvertOptions {
        strict,
        ..ConvertOptions::default()
    };

    match mode {
        Mode::Convert(input) => convert_file(&input, out_path.as_deref(), &opts),
        Mode::ConvertDir(dir) => batch_convert(&dir, out_dir.as_deref(), report, &opts),
        Mode::Dump(input) => dump_file(&input),
        Mode::Check(input) => check_file(&input),
        Mode::CheckDir(dir) => check_dir(&dir),
    }
}

fn convert_file(input: &Path, output: Option<&Path>, opts: &ConvertOptions) -> i32 {
    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: cannot read '{}': {e}", input.display());
            return 1;
        },
    };
    let parsed = match decode(&bytes) {
        Ok(p) => p,
        Err(ConvertError::NotAContainer { .. } | ConvertError::UnsupportedContainer { .. }) => {
            eprintln!(
                "Error: '{}' is not an HTBasic program container",
                input.display()
            );
            return 1;
        },
        Err(e) => {
            eprintln!("Error: decoding '{}': {e}", input.display());
            return 1;
        },
    };
    print_warnings(&parsed);
    let source = emit_source(&parsed, opts);
    let output = match output {
        Some(p) => p.to_path_buf(),
        None => input.with_extension("bas"),
    };
    if output == input {
        eprintln!(
            "Error: refusing to overwrite input container '{}'; use --out",
            input.display()
        );
        return 1;
    }
    if let Some(parent) = output.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Error: cannot create '{}': {e}", parent.display());
            return 1;
        }
    }
    if let Err(e) = fs::write(&output, &source) {
        eprintln!("Error: cannot write '{}': {e}", output.display());
        return 1;
    }
    if opts.strict && !parsed.warnings.is_empty() {
        eprintln!(
            "Strict mode: {} warning(s); exiting non-zero",
            parsed.warnings.len()
        );
        return 1;
    }
    println!("Converted '{}' -> '{}'", input.display(), output.display());
    0
}

fn batch_convert(dir: &Path, out_dir: Option<&Path>, report: bool, opts: &ConvertOptions) -> i32 {
    let out_dir = out_dir.map_or_else(|| PathBuf::from("converted"), Path::to_path_buf);
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("Error: cannot create '{}': {e}", out_dir.display());
        return 1;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error: cannot read '{}': {e}", dir.display());
            return 1;
        },
    };
    let mut converted = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let mut strict_failures = 0;
    let mut total_warnings = 0;
    let mut agg: BTreeMap<String, usize> = BTreeMap::new();
    let mut warn_agg: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error: cannot read '{}': {e}", path.display());
                failed += 1;
                continue;
            },
        };
        let parsed = match decode(&bytes) {
            Ok(p) => p,
            Err(ConvertError::NotAContainer { .. } | ConvertError::UnsupportedContainer { .. }) => {
                skipped += 1;
                continue;
            },
            Err(e) => {
                eprintln!("Error: '{}': {e}", path.display());
                failed += 1;
                continue;
            },
        };
        let source = emit_source(&parsed, opts);
        let stem = path
            .file_stem()
            .map_or("out", |s| s.to_str().unwrap_or("out"));
        let out_path = out_dir.join(format!("{stem}.bas"));
        if let Err(e) = fs::write(&out_path, &source) {
            eprintln!("Error: cannot write '{}': {e}", out_path.display());
            failed += 1;
            continue;
        }
        for (k, v) in &parsed.unknown_opcodes {
            *agg.entry(k.clone()).or_insert(0) += v;
        }
        for w in &parsed.warnings {
            *warn_agg.entry(w.message.clone()).or_insert(0) += 1;
        }
        total_warnings += parsed.warnings.len();
        converted += 1;
        if opts.strict && !parsed.warnings.is_empty() {
            eprintln!(
                "Strict: '{}' has {} warning(s)",
                path.display(),
                parsed.warnings.len()
            );
            strict_failures += 1;
        }
    }
    println!(
        "Converted {converted} file(s), skipped {skipped} non-container(s), failed {failed}, warnings {total_warnings}"
    );
    if report {
        println!();
        println!("Unknown opcode histogram:");
        let mut counts: Vec<(String, usize)> = agg.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        for (k, v) in counts {
            println!("  {k}  {v}");
        }
        println!();
        println!("Top warning messages:");
        let mut wcounts: Vec<(String, usize)> = warn_agg.into_iter().collect();
        wcounts.sort_by(|a, b| b.1.cmp(&a.1));
        for (m, v) in wcounts.into_iter().take(30) {
            println!("  {v:>5}  {m}");
        }
    }
    i32::from(failed > 0 || (opts.strict && strict_failures > 0))
}

fn dump_file(input: &Path) -> i32 {
    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: cannot read '{}': {e}", input.display());
            return 1;
        },
    };
    let parsed = match decode(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: decoding '{}': {e}", input.display());
            return 1;
        },
    };
    println!(
        "kind: {:?}, variant: {:?}, sections: {}, warnings: {}, unknown opcodes: {}",
        parsed.kind,
        parsed.variant,
        parsed.sections.len(),
        parsed.warnings.len(),
        parsed.unknown_opcodes.len()
    );
    for (si, section) in parsed.sections.iter().enumerate() {
        println!();
        println!(
            "=== section {si}: stype {}, marker {:02X} {:02X}, geometry {:?}, name_table {} entries ===",
            section.stype,
            section.marker[0],
            section.marker[1],
            section.geometry,
            section.name_table.len()
        );
        for (i, n) in section.name_table.iter().enumerate() {
            println!("  name[{i}] = {n:?}");
        }
        for line in &section.lines {
            match line {
                super::DecodedLine::Source { number, text } => {
                    println!("  {number}: {text:?}");
                },
                super::DecodedLine::Tokens {
                    number,
                    indent,
                    flag,
                    statements,
                } => {
                    println!("  {number} indent={indent} flag={flag:02X}: {statements:?}");
                },
            }
        }
    }
    print_warnings(&parsed);
    0
}

fn check_file(input: &Path) -> i32 {
    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: cannot read '{}': {e}", input.display());
            return 1;
        },
    };
    let parsed = match decode(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: decoding '{}': {e}", input.display());
            return 1;
        },
    };
    let source = emit_source(&parsed, &ConvertOptions::default());
    match crate::parser::parser::Parser::new(source).parse_program() {
        Ok(_) => {
            println!("OK   {}", input.display());
            0
        },
        Err(e) => {
            println!("FAIL {}: {e}", input.display());
            1
        },
    }
}

fn check_dir(dir: &Path) -> i32 {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error: cannot read '{}': {e}", dir.display());
            return 1;
        },
    };
    let mut ok = 0;
    let mut failed = 0;
    let mut failures: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().map_or(true, |e| e != "bas") {
            continue;
        }
        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: cannot read '{}': {e}", path.display());
                failed += 1;
                continue;
            },
        };
        match crate::parser::parser::Parser::new(source.clone()).parse_program() {
            Ok(_) => ok += 1,
            Err(e) => {
                failed += 1;
                let mut detail = String::new();
                if let crate::error::HtBasicError::ParseError { span, .. } = &e {
                    let line_no = source[..span.start.min(source.len())]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1;
                    let line_text = source
                        .lines()
                        .nth(line_no - 1)
                        .map(|l| l.trim())
                        .unwrap_or("");
                    detail = format!(" [line {line_no}: {line_text}]");
                }
                failures.push((path.display().to_string(), format!("{e}{detail}")));
            },
        }
    }
    println!("Parsed OK: {ok}, failed: {failed}");
    if !failures.is_empty() {
        println!();
        println!("Failures:");
        for (path, err) in failures {
            let first_line = err.lines().next().unwrap_or(&err);
            println!("  {path}: {first_line}");
        }
    }
    0
}

fn print_warnings(parsed: &ParsedFile) {
    const MAX: usize = 20;
    for w in parsed.warnings.iter().take(MAX) {
        eprintln!("warning @0x{:X}: {}", w.offset, w.message);
    }
    if parsed.warnings.len() > MAX {
        eprintln!("... and {} more warning(s)", parsed.warnings.len() - MAX);
    }
}

fn print_help() {
    println!("HTBasic converter — decode TransEra HTBwin95 containers to ASCII BASIC");
    println!();
    println!("Usage:");
    println!("  htbasic -c <file> [-o <out>] [--strict]");
    println!("  htbasic -C <dir> [-O <out-dir>] [--report]");
    println!("  htbasic -d <file>");
    println!("  htbasic --check <file>");
    println!("  htbasic --check-dir <dir>");
    println!();
    println!("  -c,  --convert <file>   Convert one container to ASCII source");
    println!("  -o,  --out <path>       Output path (default: <stem>.bas next to input)");
    println!("  -C,  --convert-dir <d>  Batch-convert all containers in a directory");
    println!("  -O,  --out-dir <d>      Batch output dir (default: ./converted)");
    println!("  -d,  --dump-tokens <f>  Decode only: structured token dump");
    println!("       --check <file>     Convert + parse-check (never runs)");
    println!("       --check-dir <d>    Parse-check all .bas files in dir");
    println!("       --report           With --convert-dir: unknown-opcode histogram");
    println!("       --strict           Treat decode warnings as failure");
    println!("  -h,  --help             Show this help");
}
