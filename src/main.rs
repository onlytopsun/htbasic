mod analyzer;
mod converter;
mod error;
mod lexer;
mod parser;
mod program;
mod repl;
mod runtime;

use crate::parser::parser::Parser;
use crate::repl::run_repl;
use crate::runtime::bytecode::{Compiler, VM};
use crate::runtime::interpreter::Interpreter;
use std::env;
use std::fs;
use std::process;

/// True when any converter-only flag is present — dispatch to the converter CLI.
fn converter_requested(args: &[String]) -> bool {
    const FLAGS: &[&str] = &[
        "-c",
        "--convert",
        "-C",
        "--convert-dir",
        "-d",
        "--dump-tokens",
        "--check",
        "--check-dir",
        "--strict",
        "--report",
        "-o",
        "--out",
        "-O",
        "--out-dir",
    ];
    args.iter().any(|a| FLAGS.contains(&a.as_str()))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if converter_requested(&args[1..]) {
        converter::run(&args[1..]);
    }

    let mut use_bytecode = false;
    let mut use_repl = false;
    let mut filename: Option<&str> = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "--bytecode" | "-b" => use_bytecode = true,
            "--repl" | "-i" => use_repl = true,
            "--help" | "-h" => {
                eprintln!("Usage: htbasic [options] [file.bas]");
                eprintln!();
                eprintln!("HTBasic / Rocky Mountain BASIC Interpreter v0.4.0");
                eprintln!("  file.bas          Execute a BASIC program file");
                eprintln!("  --repl, -i        Start interactive REPL (default if no file)");
                eprintln!("  --bytecode, -b    Use bytecode VM (experimental)");
                eprintln!("  --help, -h        Show this help");
                eprintln!();
                eprintln!("TransEra container conversion (see 'htbasic --convert --help'):");
                eprintln!("  --convert <f>     Decode a HTBwin95 .prg/.bas container to source");
                eprintln!("  --convert-dir <d> Batch-convert all containers in a directory");
                eprintln!("  --dump-tokens <f> Structured decode dump");
                eprintln!("  --check <f>       Convert + parse-check a container");
                eprintln!("  --check-dir <d>   Parse-check all .bas files in a directory");
                process::exit(0);
            },
            _ if !arg.starts_with('-') => filename = Some(arg),
            _ => {},
        }
    }

    // Start REPL if --repl specified or no file given
    if use_repl || filename.is_none() {
        run_repl();
        return;
    }

    let filename = filename.unwrap();
    let source = match fs::read_to_string(filename) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", filename, e);
            process::exit(1);
        },
    };

    // Lex + Parse
    let mut parser = Parser::new(source);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            process::exit(1);
        },
    };

    if use_bytecode {
        let compiler = Compiler::new();
        let chunk = compiler.compile(program);
        let mut vm = VM::new(chunk);
        match vm.run() {
            Ok(output) => {
                for line in output {
                    println!("{}", line);
                }
            },
            Err(e) => {
                eprintln!("Runtime error: {}", e);
                process::exit(1);
            },
        }
    } else {
        let mut interpreter = Interpreter::new(program);
        match interpreter.run() {
            Ok(output) => {
                for line in output {
                    println!("{}", line);
                }
            },
            Err(e) => {
                eprintln!("Runtime error: {}", e);
                process::exit(1);
            },
        }
    }
}
