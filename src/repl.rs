/// HTBasic REPL — interactive Read-Eval-Print-Loop.
///
/// Supports:
/// - Immediate mode: statements execute directly
/// - Program mode: line-numbered input stored in buffer
/// - RUN: compile and execute the program buffer
/// - LIST, SAVE, LOAD, REN, SCRATCH, HELP, EDIT, AUTO, DEL
/// - Batch mode: when stdin is not a TTY, reads lines directly
use crate::parser::parser::Parser;
use crate::program::ProgramBuffer;
use crate::runtime::interpreter::Interpreter;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::io::{self, BufRead};

const PROMPT: &str = "HTBasic> ";
const VERSION: &str = "HTBasic Interpreter v0.4.0 — REPL Edition";

/// Check if stdin is a TTY (interactive terminal).
#[allow(dead_code)]
fn is_tty() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        unsafe { libc::isatty(io::stdin().as_raw_fd()) != 0 }
    }
    #[cfg(windows)]
    {
        // On Windows, use console API — simplified check
        true // Assume TTY on Windows for now; fallback to batch if rustyline fails
    }
}

pub fn run_repl() {
    // Try interactive REPL first
    if run_interactive() {
        return;
    }
    // Fall back to batch mode
    run_batch();
}

fn run_interactive() -> bool {
    let mut editor = match DefaultEditor::new() {
        Ok(e) => e,
        Err(_) => return false,
    };

    let _ = editor.load_history(".htbasic_history");

    println!("{}", VERSION);
    println!("Type HELP for commands, Ctrl+C to interrupt, Ctrl+D to exit.\n");

    let mut buffer = ProgramBuffer::new();
    let mut interpreter: Option<Interpreter> = None;

    loop {
        match editor.readline(PROMPT) {
            Ok(line) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(&line);

                // Handle REPL commands
                if handle_command(&line, &mut buffer, &mut interpreter, &mut editor) {
                    continue;
                }

                // Try program line input (starts with number)
                if let Some((num, text)) = ProgramBuffer::parse_input(&line) {
                    if text.is_empty() {
                        // Just a line number — delete the line
                        if buffer.delete(num).is_some() {
                            println!("  Deleted line {}", num);
                        }
                    } else {
                        buffer.put(num, &text);
                        println!("  Line {} stored", num);
                    }
                    continue;
                }

                // Immediate mode: execute the statement
                execute_immediate(&line, &mut buffer, &mut interpreter);
            },
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                println!("Type EXIT or press Ctrl+D to quit.");
                continue;
            },
            Err(ReadlineError::Eof) => {
                println!("\nGoodbye.");
                break;
            },
            Err(err) => {
                eprintln!("Error: {}", err);
                break;
            },
        }
    }

    let _ = editor.save_history(".htbasic_history");
    true
}

/// Handle built-in REPL commands. Returns true if the line was a command.
fn handle_command(
    line: &str,
    buffer: &mut ProgramBuffer,
    interpreter: &mut Option<Interpreter>,
    editor: &mut DefaultEditor,
) -> bool {
    let upper = line.trim().to_uppercase();
    let parts: Vec<&str> = line.trim().splitn(2, ' ').collect();
    let cmd = parts[0].to_uppercase();
    let arg = parts.get(1).map(|s| s.trim());

    match cmd.as_str() {
        "RUN" => {
            if buffer.is_empty() {
                println!("  No program in memory.");
                return true;
            }
            let source = buffer.to_source();
            let mut parser = Parser::new(source);
            match parser.parse_program() {
                Ok(program) => {
                    let mut interp = Interpreter::new(program);
                    match interp.run() {
                        Ok(output) => {
                            // Keep interpreter for immediate mode variable persistence
                            *interpreter = Some(interp);
                            for line in output {
                                if !line.is_empty() {
                                    println!("{}", line);
                                }
                            }
                        },
                        Err(e) => eprintln!("Runtime error: {}", e),
                    }
                },
                Err(e) => eprintln!("Parse error: {}", e),
            }
            true
        },

        "LIST" => {
            if buffer.is_empty() {
                println!("  No program in memory.");
                return true;
            }
            let range = parse_list_range(arg.unwrap_or(""));
            let lines = buffer.list(range);
            for line in &lines {
                println!("{}", line);
            }
            if lines.is_empty() {
                println!("  (no matching lines)");
            }
            true
        },

        "SAVE" => {
            let filename = arg.unwrap_or("program.bas");
            let result = if filename.ends_with(".htb") || filename.ends_with(".HTB") {
                buffer.save_binary(filename)
            } else {
                buffer.save(filename)
            };
            match result {
                Ok(()) => println!("  Program saved to {}", filename),
                Err(e) => eprintln!("  Error saving: {}", e),
            }
            true
        },

        "GET" => {
            let filename = arg.unwrap_or("program.htb");
            match buffer.get_binary(filename) {
                Ok(count) => println!("  Got {} lines from {}", count, filename),
                Err(e) => eprintln!("  Error loading: {}", e),
            }
            true
        },

        "LOAD" => {
            let filename = arg.unwrap_or("program.bas");
            match buffer.load(filename) {
                Ok(count) => println!("  Loaded {} lines from {}", count, filename),
                Err(e) => eprintln!("  Error loading: {}", e),
            }
            true
        },

        "REN" | "RENUMBER" => {
            let start: u32 = arg.and_then(|s| s.parse().ok()).unwrap_or(10);
            buffer.renumber(start, 10);
            println!("  Program renumbered starting at {}", start);
            true
        },

        "SCRATCH" => {
            buffer.clear();
            *interpreter = None;
            println!("  Program and variables cleared.");
            true
        },

        "EDIT" => {
            if let Some(line_num_str) = arg {
                if let Ok(num) = line_num_str.parse::<u32>() {
                    if let Some(text) = buffer.get(num) {
                        // Pre-fill the line for editing
                        let prefill = format!("{} {}", num, text);
                        println!(
                            "  Editing line {} (press Enter to keep, type new text to replace)",
                            num
                        );
                        match editor.readline_with_initial("EDIT> ", (&prefill, "")) {
                            Ok(new_line) => {
                                let new_line = new_line.trim().to_string();
                                if new_line.is_empty() {
                                    buffer.delete(num);
                                    println!("  Line {} deleted", num);
                                } else if let Some((_, text)) =
                                    ProgramBuffer::parse_input(&new_line)
                                {
                                    buffer.put(num, &text);
                                    println!("  Line {} updated", num);
                                } else {
                                    buffer.put(num, &new_line);
                                    println!("  Line {} updated", num);
                                }
                            },
                            Err(_) => {},
                        }
                    } else {
                        println!("  Line {} not found.", num);
                    }
                }
            } else {
                println!("  Usage: EDIT <line_number>");
            }
            true
        },

        "AUTO" => {
            let start: u32 = arg.and_then(|s| s.parse().ok()).unwrap_or(10);
            println!(
                "  AUTO mode starting at {} (press Enter on empty line to exit)",
                start
            );
            let mut next = start;
            loop {
                match editor
                    .readline_with_initial(&format!("{:04}> ", next), (&format!("{} ", next), ""))
                {
                    Ok(line) => {
                        let line = line.trim().to_string();
                        if line.is_empty() {
                            println!("  AUTO ended.");
                            break;
                        }
                        if let Some((num, text)) = ProgramBuffer::parse_input(&line) {
                            buffer.put(num, &text);
                            next = num + 10;
                        } else {
                            buffer.put(next, &line);
                            next += 10;
                        }
                    },
                    Err(_) => break,
                }
            }
            true
        },

        "MERGE" => {
            let filename = arg.unwrap_or("program.bas");
            match buffer.merge(filename) {
                Ok(count) => println!("  Merged {} lines from {}", count, filename),
                Err(e) => eprintln!("  Error merging: {}", e),
            }
            true
        },

        "DEL" | "DELETE" => {
            let range = parse_list_range(arg.unwrap_or(""));
            let count = buffer.delete_range(range.unwrap_or((None, None)));
            println!("  Deleted {} lines.", count);
            true
        },

        "HELP" => {
            println!("  Commands:");
            println!("    RUN              Execute the program in memory");
            println!("    LIST [range]     List program lines (e.g., LIST 100-200)");
            println!("    SAVE [file]      Save as .BAS (or .HTB binary if extension is .htb)");
            println!("    LOAD [file]      Load from .BAS file");
            println!("    GET [file]       Load from .HTB binary file");
            println!("    REN [start]      Renumber lines starting at start (default: 10)");
            println!("    SCRATCH          Clear program and variables");
            println!("    EDIT <line>      Edit a specific line");
            println!("    AUTO [start]     Auto-number new lines");
            println!("    DEL <range>      Delete lines (e.g., DEL 100-200)");
            println!("    MERGE [file]     Merge lines from file into current program");
            println!("    HELP             Show this help");
            println!("    EXIT / QUIT      Exit the interpreter");
            println!();
            println!("  Program entry:");
            println!("    <number> <stmt>  Add/replace a program line");
            println!("    <number>         Delete a program line");
            println!("    <statement>      Execute immediately (immediate mode)");
            true
        },

        "EXIT" | "QUIT" | "BYE" => {
            println!("Goodbye.");
            std::process::exit(0);
        },

        _ => false, // Not a command, try as statement
    }
}

/// Execute a statement in immediate mode.
fn execute_immediate(line: &str, buffer: &ProgramBuffer, interpreter: &mut Option<Interpreter>) {
    // Build source: program context + immediate statement
    // For immediate mode, we need the program context (SUB/FN definitions)
    // plus the immediate statement
    let program_source = buffer.to_source_no_end();
    let source = if program_source.trim().is_empty() {
        format!("{}\nEND\n", line)
    } else {
        format!("{}\n{}\nEND\n", program_source, line)
    };

    let mut parser = Parser::new(source);
    match parser.parse_program() {
        Ok(program) => {
            // Execute with a fresh interpreter for each immediate statement.
            // Variable persistence across statements is achieved by including
            // the full program context each time.
            let mut interp = Interpreter::new(program);
            match interp.run() {
                Ok(output) => {
                    for line in output {
                        if !line.is_empty() {
                            println!("{}", line);
                        }
                    }
                    *interpreter = Some(interp);
                },
                Err(e) => {
                    eprintln!("Error: {}", e);
                },
            }
        },
        Err(e) => {
            eprintln!("Parse error: {}", e);
        },
    }
}

/// Parse a LIST-style range: "100-200", "100-", "-200", or "100"
fn parse_list_range(arg: &str) -> Option<(Option<u32>, Option<u32>)> {
    let arg = arg.trim();
    if arg.is_empty() {
        return None;
    }
    if arg == "-" {
        return None; // entire program
    }

    if let Some(dash_pos) = arg.find('-') {
        let before = &arg[..dash_pos];
        let after = &arg[dash_pos + 1..];
        let start = if before.is_empty() {
            None
        } else {
            before.parse::<u32>().ok()
        };
        let end = if after.is_empty() {
            None
        } else {
            after.parse::<u32>().ok()
        };
        Some((start, end))
    } else {
        // Single number — treat as start
        if let Ok(n) = arg.parse::<u32>() {
            Some((Some(n), Some(n)))
        } else {
            None
        }
    }
}

/// Batch mode — read lines from stdin (non-interactive, for testing/piping).
fn run_batch() {
    println!("{}", VERSION);
    println!("(batch mode — reading from stdin)\n");

    let stdin = io::stdin();
    let mut buffer = ProgramBuffer::new();
    let mut interpreter: Option<Interpreter> = None;

    for line_result in stdin.lock().lines() {
        let line = match line_result {
            Ok(l) => l.trim().to_string(),
            Err(_) => break,
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle commands
        if handle_command(
            &line,
            &mut buffer,
            &mut interpreter,
            &mut DefaultEditor::new().unwrap_or_else(|_| DefaultEditor::new().unwrap()),
        ) {
            continue;
        }

        // Try program line input
        if let Some((num, text)) = ProgramBuffer::parse_input(&line) {
            if text.is_empty() {
                buffer.delete(num);
            } else {
                buffer.put(num, &text);
            }
            continue;
        }

        // Immediate mode
        execute_immediate(&line, &buffer, &mut interpreter);
    }
}
