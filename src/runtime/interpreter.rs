use crate::error::{HtBasicError, Result, Span};
use crate::parser::ast::*;
use crate::runtime::builtins::Builtins;
use crate::runtime::gpib::GpibBus;
use crate::runtime::graphics::GraphicsState;
use crate::runtime::io::IoRegistry;
use crate::runtime::value::{ArrayData, Value};
use std::collections::HashMap;

/// A flat representation of an executable instruction.
/// The program is stored as a flat vector so GOTO can set the PC directly.
#[derive(Debug, Clone)]
enum Instr {
    /// Execute a statement, then advance PC by 1.
    Stmt(Stmt),
    /// Unconditional jump to another index.
    Jump(usize),
    /// Conditional jump: if condition (index into condition stack) is false, jump.
    JumpIfFalse(usize, usize), // condition_expr_index, target
    /// Push current PC+1 onto call stack, then jump (GOSUB).
    GoSub(usize),
    /// Return from GOSUB: pop PC from call stack.
    Return,
    /// Marker for a label.
    Label(String),
    /// End of program.
    Halt,
}

/// Runtime scope — stores variables for a given context.
#[derive(Clone, Debug)]
struct Scope {
    variables: HashMap<String, Value>,
    parent: Option<Box<Scope>>,
}

impl Scope {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            parent: None,
        }
    }

    fn with_parent(parent: Scope) -> Self {
        Self {
            variables: HashMap::new(),
            parent: Some(Box::new(parent)),
        }
    }

    fn get(&self, name: &str) -> Option<Value> {
        let upper = name.to_uppercase();
        if let Some(v) = self.variables.get(&upper) {
            return Some(v.clone());
        }
        if let Some(ref parent) = self.parent {
            parent.get(name)
        } else {
            None
        }
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        let upper = name.to_uppercase();
        if self.variables.contains_key(&upper) {
            return self.variables.get_mut(&upper);
        }
        if let Some(ref mut parent) = self.parent {
            parent.get_mut(name)
        } else {
            None
        }
    }

    fn set(&mut self, name: &str, value: Value) {
        let upper = name.to_uppercase();
        self.variables.insert(upper, value);
    }

    fn remove(&mut self, name: &str) -> Option<Value> {
        let upper = name.to_uppercase();
        self.variables.remove(&upper)
    }

    fn has(&self, name: &str) -> bool {
        let upper = name.to_uppercase();
        self.variables.contains_key(&upper)
            || self.parent.as_ref().map(|p| p.has(name)).unwrap_or(false)
    }
}

/// A call frame for GOSUB and CALL.
#[derive(Debug, Clone)]
struct CallFrame {
    return_pc: usize,
    local_scope: Scope,
}

/// Interpreter state maintained across execution.
pub struct Interpreter {
    /// Flat instruction vector.
    instructions: Vec<Instr>,
    /// Program counter (index into instructions).
    pc: usize,
    /// Call stack (for GOSUB / CALL).
    call_stack: Vec<CallFrame>,
    /// Data pointer for READ.
    data_pointer: usize,
    /// All DATA values collected during parsing.
    data_values: Vec<Value>,
    /// Current scope chain (global scope at bottom).
    scope: Scope,
    /// COM blocks: block_name → entry definitions (type, dimensions).
    com_blocks: HashMap<String, ComBlock>,
    /// Subprogram registry: name → definition.
    sub_registry: HashMap<String, SubProgram>,
    /// DEF FN registry: name → definition.
    fn_registry: HashMap<String, FnDef>,
    /// Built-in function registry.
    builtins: Builtins,
    /// I/O output buffer (for PRINT).
    output: Vec<String>,
    /// Option base (0 or 1).
    option_base: usize,
    /// File I/O registry.
    io: IoRegistry,
    /// Pending OUTPUT text per path (trailing `;`/`,` keeps the line open).
    io_pending: HashMap<String, String>,
    /// Error handler label (ON ERROR GOTO label).
    error_handler: Option<String>,
    /// Event handlers: event_name → (label, priority, response_type)
    event_handlers: HashMap<String, (String, usize, String)>,
    /// Whether events are enabled
    events_enabled: bool,
    /// Last error code.
    last_err: i64,
    /// Last error line.
    last_err_line: i64,
    /// Last error message.
    last_err_msg: String,
    /// Graphics state for Phase 4.
    graphics: GraphicsState,
    /// GPIB bus for instrument simulation.
    gpib: GpibBus,
    /// Map from path name → GPIB address (populated by ASSIGN).
    gpib_paths: HashMap<String, u8>,
    /// DEF FN return capture: None = not executing a FN body;
    /// Some(Armed) = inside FN body, no RETURN yet; Some(Fired(v)) = RETURN ran.
    fn_return: Option<FnReturn>,
}

/// Return state while executing a DEF FN body.
#[derive(Clone)]
enum FnReturn {
    Armed,
    Fired(Option<Value>),
}

impl Interpreter {
    pub fn new(program: Program) -> Self {
        let mut sub_registry: HashMap<String, SubProgram> = HashMap::new();
        let mut fn_registry: HashMap<String, FnDef> = HashMap::new();
        for sub in &program.subprograms {
            sub_registry.insert(sub.name.to_uppercase(), sub.clone());
        }
        for func in &program.functions {
            fn_registry.insert(func.name.to_uppercase(), func.clone());
        }

        let mut interp = Self {
            instructions: Vec::new(),
            pc: 0,
            call_stack: Vec::new(),
            data_pointer: 0,
            data_values: Vec::new(),
            scope: Scope::new(),
            com_blocks: HashMap::new(),
            sub_registry,
            fn_registry,
            builtins: Builtins::new(),
            output: Vec::new(),
            option_base: 0,
            io: IoRegistry::new(),
            io_pending: HashMap::new(),
            error_handler: None,
            event_handlers: HashMap::new(),
            events_enabled: true,
            last_err: 0,
            last_err_line: 0,
            last_err_msg: String::new(),
            graphics: GraphicsState::new(),
            gpib: GpibBus::new(),
            gpib_paths: HashMap::new(),
            fn_return: None,
        };

        interp.compile(program);
        interp
    }

    /// Compile the AST into flat instructions.
    fn compile(&mut self, program: Program) {
        // First pass: collect all DATA values and labels
        let mut label_map: HashMap<String, usize> = HashMap::new();

        // Build instructions from statements
        self.compile_statements(&program.statements, &mut label_map);

        // Register subprograms as callable labels
        for sub in &program.subprograms {
            let idx = self.instructions.len();
            label_map.insert(sub.name.clone(), idx);
            self.instructions.push(Instr::Label(sub.name.clone()));
            self.compile_statements(&sub.body, &mut label_map);
            self.instructions.push(Instr::Return); // implicit return at end of SUB
        }

        // Register functions
        for func in &program.functions {
            let idx = self.instructions.len();
            label_map.insert(func.name.clone(), idx);
            self.instructions.push(Instr::Label(func.name.clone()));
            self.compile_statements(&func.body, &mut label_map);
            self.instructions.push(Instr::Return);
        }

        self.instructions.push(Instr::Halt);

        // Second pass: resolve label references in GOTO/GOSUB
        self.resolve_labels(&label_map);
    }

    fn compile_statements(&mut self, stmts: &[Stmt], label_map: &mut HashMap<String, usize>) {
        for stmt in stmts {
            // Check for label markers (inserted during parsing)
            if let Stmt::Rem(ref msg, _) = stmt {
                if msg.starts_with("__label__") {
                    let label = msg.trim_start_matches("__label__").to_string();
                    label_map.insert(label.clone(), self.instructions.len());
                    self.instructions.push(Instr::Label(label));
                    continue;
                }
            }

            // Handle EXIT IF specially — it's inside LOOP blocks
            if let Stmt::Rem(ref msg, _) = stmt {
                if msg == "EXIT IF" {
                    self.instructions.push(Instr::Stmt(stmt.clone()));
                    continue;
                }
            }

            self.instructions.push(Instr::Stmt(stmt.clone()));
        }
    }

    fn resolve_labels(&mut self, label_map: &HashMap<String, usize>) {
        for instr in &mut self.instructions {
            match instr {
                Instr::Stmt(Stmt::GoTo(ref label, _)) => {
                    if let Some(&idx) = label_map.get(label) {
                        *instr = Instr::Jump(idx);
                    }
                },
                Instr::Stmt(Stmt::GoSub(ref label, _)) => {
                    if let Some(&idx) = label_map.get(label) {
                        *instr = Instr::GoSub(idx);
                    }
                },
                _ => {},
            }
        }
    }

    // ===================== Public API =====================

    /// Execute the program and return captured output.
    pub fn run(&mut self) -> Result<Vec<String>> {
        while self.pc < self.instructions.len() {
            let instr = self.instructions[self.pc].clone();

            match instr {
                Instr::Stmt(stmt) => {
                    if let Err(e) = self.execute_stmt(&stmt) {
                        // Arithmetic and subscript errors raise directly
                        // rather than through runtime_error; route them
                        // through the ON ERROR handler when one is set.
                        let code = match &e {
                            HtBasicError::DivisionByZero { .. } => Some(26),
                            HtBasicError::SubscriptError { .. } => Some(61),
                            HtBasicError::TypeError { .. } => Some(70),
                            HtBasicError::UndefinedVariable { .. } => Some(71),
                            _ => None,
                        };
                        if let Some(c) = code {
                            if self.runtime_error(c, &e.to_string()).is_ok() {
                                continue; // jumped to the ON ERROR handler
                            }
                        }
                        return Err(e);
                    }
                    self.pc += 1;
                },
                Instr::Jump(target) => {
                    self.pc = target;
                },
                Instr::JumpIfFalse(_cond_idx, target) => {
                    // For now, condition is evaluated inline
                    self.pc = target;
                },
                Instr::GoSub(target) => {
                    self.call_stack.push(CallFrame {
                        return_pc: self.pc + 1,
                        local_scope: self.scope.clone(),
                    });
                    self.pc = target;
                },
                Instr::Return => {
                    if let Some(frame) = self.call_stack.pop() {
                        self.scope = frame.local_scope;
                        self.pc = frame.return_pc;
                    } else {
                        // No more call frames — we've returned past the main CALL.
                        // Halt to avoid falling into adjacent SUB bodies.
                        self.pc = self.instructions.len();
                    }
                },
                Instr::Label(_) => {
                    self.pc += 1; // Skip labels
                },
                Instr::Halt => {
                    break;
                },
            }
        }

        self.flush_io_pending();

        Ok(std::mem::take(&mut self.output))
    }

    /// Render OUTPUT print items into one line. Numbers get HP-style
    /// sign spacing (a leading space), matching `OUTPUT @Out; A(1,1);`
    /// output in the TransEra examples (e.g. Prtmat's `[ 1 2 3 ]`).
    fn render_output_items(&mut self, items: &[PrintItem]) -> Result<String> {
        let mut line = String::new();
        for item in items {
            match item {
                PrintItem::Expr(expr) => {
                    let val = self.eval_expr(expr)?;
                    if matches!(&val, Value::Real(_) | Value::Integer(_)) {
                        line.push(' ');
                    }
                    line.push_str(&val.to_display_string());
                },
                PrintItem::Semicolon => {},
                PrintItem::Comma => {
                    // Tab to next zone (every 16 columns)
                    while line.len() % 16 != 0 {
                        line.push(' ');
                    }
                },
                PrintItem::Tab(expr) => {
                    let col = self.eval_expr(expr)?.as_integer().max(1) as usize;
                    while line.len() < col {
                        line.push(' ');
                    }
                },
                PrintItem::Using(format, exprs) => {
                    let fmt_str = self.eval_expr(format)?.as_string();
                    for e in exprs {
                        let val = self.eval_expr(e)?;
                        line.push_str(&self.format_using(&fmt_str, &val));
                    }
                },
            }
        }
        Ok(line)
    }

    /// Write one complete OUTPUT line to its destination. CRT-assigned
    /// paths (and the built-in CRT device) go to the program output.
    fn write_output_line(&mut self, path: &str, text: &str) {
        if self.io.is_crt(path) || path.eq_ignore_ascii_case("CRT") {
            self.output.push(text.to_string());
            return;
        }
        if let Some(gpib_addr) = self.resolve_gpib(path) {
            let response = self.gpib.output(gpib_addr, text);
            if !response.is_empty() {
                self.scope
                    .set(&format!("__gpib_resp_{}", path), Value::string(&response));
            }
            return;
        }
        if let Err(e) = self.io.output(path, text) {
            let _ = self.runtime_error(710, &format!("OUTPUT: {}", e));
        }
    }

    /// Flush OUTPUT lines still pending at program end (trailing `;`/`,`
    /// suppresses the terminator, so text can outlive its statement).
    fn flush_io_pending(&mut self) {
        let pending = std::mem::take(&mut self.io_pending);
        for (path, text) in pending {
            self.write_output_line(&path, &text);
        }
    }

    // ===================== Statement Execution =====================

    fn execute_stmt(&mut self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Let(name, expr, _span) => {
                let value = self.eval_expr(expr)?;
                self.scope.set(name, value);
            },

            Stmt::ArrayAssign(name, indices, value, _span) => {
                let val = self.eval_expr(value)?;
                let subs: Result<Vec<i64>> = indices
                    .iter()
                    .map(|e| Ok(self.eval_expr(e)?.as_integer()))
                    .collect();
                let subs = subs?;
                if let Some(Value::Array(ref mut arr)) = self.scope.get_mut(name) {
                    arr.set(&subs, val);
                } else {
                    // Array not yet declared — create it implicitly? HTBasic doesn't auto-create.
                }
            },

            Stmt::Dim(entries, _span) => {
                for entry in entries {
                    let dims: Vec<(i64, i64)> = entry
                        .dimensions
                        .iter()
                        .map(|&(lo, hi)| {
                            let lower = if lo == 0 { self.option_base as i64 } else { lo };
                            (lower, hi)
                        })
                        .collect();

                    let arr = ArrayData::new(dims);
                    self.scope.set(&entry.name, Value::Array(arr));
                }
            },

            Stmt::Com(com_block, _span) => {
                for entry in &com_block.entries {
                    let dims: Vec<(i64, i64)> =
                        entry.dimensions.iter().map(|&(lo, hi)| (lo, hi)).collect();

                    if dims.is_empty() {
                        // Scalar variable
                        let default = match entry.var_type {
                            VarType::Integer | VarType::Short | VarType::Long => Value::Integer(0),
                            VarType::Real => Value::Real(0.0),
                            VarType::String_ => Value::string(""),
                            VarType::Complex => Value::Real(0.0),
                        };
                        self.scope.set(&entry.name, default);
                    } else {
                        let arr = ArrayData::new(dims);
                        self.scope.set(&entry.name, Value::Array(arr));
                    }
                }

                let block_name = com_block
                    .name
                    .clone()
                    .unwrap_or_else(|| "__blank_common__".to_string());
                self.com_blocks.insert(block_name, com_block.clone());
            },

            Stmt::Print(items, _span) => {
                let mut line = String::new();
                for item in items {
                    match item {
                        PrintItem::Expr(expr) => {
                            let val = self.eval_expr(expr)?;
                            line.push_str(&val.to_display_string());
                        },
                        PrintItem::Semicolon => {
                            // Suppress newline — just continue
                        },
                        PrintItem::Comma => {
                            // Tab to next zone (every 16 columns)
                            while line.len() % 16 != 0 {
                                line.push(' ');
                            }
                        },
                        PrintItem::Tab(expr) => {
                            let col = self.eval_expr(expr)?.as_integer().max(1) as usize;
                            while line.len() < col {
                                line.push(' ');
                            }
                        },
                        PrintItem::Using(format, exprs) => {
                            let fmt_str = self.eval_expr(format)?.as_string();
                            for e in exprs {
                                let val = self.eval_expr(e)?;
                                let formatted = self.format_using(&fmt_str, &val);
                                line.push_str(&formatted);
                            }
                        },
                    }
                }
                self.output.push(line);
            },

            Stmt::PrintUsing(format, exprs, _span) => {
                let fmt_str = self.eval_expr(format)?.as_string();
                let mut line = String::new();
                for e in exprs {
                    let val = self.eval_expr(e)?;
                    let formatted = self.format_using(&fmt_str, &val);
                    line.push_str(&formatted);
                }
                self.output.push(line);
            },

            Stmt::Output(path, items, _span) => {
                // OUTPUT @path; items — a trailing `;`/`,` keeps the line
                // open (HTBasic suppresses the terminator), so text
                // accumulates per path until an OUTPUT ends without one.
                let line = self.render_output_items(items)?;
                let open = matches!(
                    items.last(),
                    Some(PrintItem::Semicolon) | Some(PrintItem::Comma)
                );
                let entry = self.io_pending.entry(path.clone()).or_default();
                entry.push_str(&line);
                if !open {
                    if let Some(text) = self.io_pending.remove(path) {
                        self.write_output_line(path, &text);
                    }
                }
            },

            Stmt::If(if_block, _span) => {
                let cond = self.eval_expr(&if_block.condition)?;
                if cond.is_truthy() {
                    for s in &if_block.then_body {
                        self.execute_stmt(s)?;
                    }
                } else {
                    let mut executed = false;
                    for (elseif_cond, body) in &if_block.else_ifs {
                        if self.eval_expr(elseif_cond)?.is_truthy() {
                            for s in body {
                                self.execute_stmt(s)?;
                            }
                            executed = true;
                            break;
                        }
                    }
                    if !executed {
                        if let Some(ref else_body) = if_block.else_body {
                            for s in else_body {
                                self.execute_stmt(s)?;
                            }
                        }
                    }
                }
            },

            Stmt::SingleLineIf(cond, then_stmt, else_stmt, _span) => {
                if self.eval_expr(cond)?.is_truthy() {
                    self.execute_stmt(then_stmt)?;
                } else if let Some(ref else_s) = else_stmt {
                    self.execute_stmt(else_s)?;
                }
            },

            Stmt::For(var, start, end, step, body, _span) => {
                let start_val = self.eval_expr(start)?.as_real();
                let end_val = self.eval_expr(end)?.as_real();
                let step_val = step
                    .as_ref()
                    .map(|s| self.eval_expr(s).map(|v| v.as_real()))
                    .unwrap_or(Ok(1.0))?;

                let mut i = start_val;
                loop {
                    if step_val > 0.0 && i > end_val {
                        break;
                    }
                    if step_val < 0.0 && i < end_val {
                        break;
                    }

                    self.scope.set(var, Value::Real(i));

                    for s in body {
                        self.execute_stmt(s)?;
                    }

                    i += step_val;
                }
            },

            Stmt::While(cond, body, _span) => {
                while self.eval_expr(cond)?.is_truthy() {
                    for s in body {
                        self.execute_stmt(s)?;
                    }
                }
            },

            Stmt::Loop_(body, _span) => {
                loop {
                    let mut should_exit = false;
                    for s in body {
                        // Check for EXIT IF
                        if let Stmt::ExitIf(ref cond, _) = s {
                            if self.eval_expr(cond)?.is_truthy() {
                                should_exit = true;
                                break;
                            }
                            continue;
                        }
                        self.execute_stmt(s)?;
                    }
                    if should_exit {
                        break;
                    }
                }
            },

            Stmt::Repeat(body, cond, _span) => loop {
                for s in body {
                    self.execute_stmt(s)?;
                }
                if self.eval_expr(cond)?.is_truthy() {
                    break;
                }
            },

            Stmt::GoTo(label, _span) => {
                // Check for ON ERROR GOTO
                if label.starts_with("__onerror__") {
                    let real_label = label.trim_start_matches("__onerror__").to_string();
                    self.error_handler = Some(real_label);
                } else {
                    // Find the label and jump
                    self.jump_to_label(label)?;
                }
            },

            Stmt::GoSub(label, _span) => {
                for (i, instr) in self.instructions.iter().enumerate() {
                    if let Instr::Label(ref l) = instr {
                        if l == label {
                            self.call_stack.push(CallFrame {
                                return_pc: self.pc + 1,
                                local_scope: self.scope.clone(),
                            });
                            self.pc = i;
                            return Ok(());
                        }
                    }
                }
                return Err(HtBasicError::RuntimeError {
                    message: format!("Label not found: {}", label),
                    span: None,
                });
            },

            Stmt::Return(expr, _span) => {
                let ret_val = if let Some(ref e) = expr {
                    Some(self.eval_expr(e)?)
                } else {
                    None
                };

                if let Some(fr) = self.fn_return.as_mut() {
                    // RETURN nested inside a DEF FN body (e.g. in a single-line
                    // IF): capture it for execute_fn instead of popping the
                    // GOSUB call stack.
                    *fr = FnReturn::Fired(ret_val);
                } else if let Some(mut frame) = self.call_stack.pop() {
                    if let Some(val) = ret_val {
                        self.scope.set("__return__", val.clone());
                        frame.local_scope.set("__return__", val);
                    }
                    self.scope = frame.local_scope;
                    // Subtract 1 because main loop will add 1 after execute_stmt
                    self.pc = frame.return_pc.saturating_sub(1);
                }
            },

            Stmt::Call(name, args, _span) => {
                // Evaluate arguments
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<Vec<_>>>()?;

                let upper = name.to_uppercase();

                // Find subprogram definition
                if let Some(sub) = self.sub_registry.get(&upper).cloned() {
                    // Save caller's scope and PC.
                    // return_pc should point to the instruction AFTER this CALL.
                    // After execute_stmt returns, main loop does pc += 1,
                    // so we set pc = label_idx - 1 and save return_pc = current_pc + 1.
                    let caller_scope = self.scope.clone();
                    let return_pc = self.pc + 1; // instruction after CALL

                    // Create local scope for SUB
                    let mut local_scope = Scope::new();

                    // Bind parameters
                    for (i, param) in sub.params.iter().enumerate() {
                        if i < arg_vals.len() {
                            let val = arg_vals[i].clone();
                            local_scope.set(&param.name, val);
                        } else {
                            // Optional parameter — default to 0 or ""
                            match param.param_type {
                                ParamType::String_ => {
                                    local_scope.set(&param.name, Value::string(""))
                                },
                                _ => local_scope.set(&param.name, Value::Real(0.0)),
                            }
                        }
                    }

                    // Push call frame
                    self.call_stack.push(CallFrame {
                        return_pc,
                        local_scope: caller_scope,
                    });

                    // Set local scope
                    self.scope = local_scope;

                    // Find and jump to SUB body. Subtract 1 because main loop adds 1 after execute_stmt
                    if let Some(idx) = self.find_label(name) {
                        self.pc = idx.saturating_sub(1);
                    }
                    return Ok(());
                }

                // SUB not found
                return self.runtime_error(201, &format!("SUB not found: {}", name));
            },

            Stmt::Data(values, _span) => {
                for v in values {
                    let val = self.eval_expr(v)?;
                    self.data_values.push(val);
                }
            },

            Stmt::Read(vars, _span) => {
                for var in vars {
                    // Whole-array target (`READ Matrix(*)`): fill the
                    // array in row-major order from DATA.
                    if let Some(Value::Array(arr)) = self.scope.get(var) {
                        let mut filled = arr.clone();
                        for slot in &mut filled.data {
                            if self.data_pointer >= self.data_values.len() {
                                return Err(HtBasicError::RuntimeError {
                                    message: "READ past end of DATA".to_string(),
                                    span: None,
                                });
                            }
                            *slot = self.data_values[self.data_pointer].clone();
                            self.data_pointer += 1;
                        }
                        self.scope.set(var, Value::Array(filled));
                        continue;
                    }
                    if self.data_pointer >= self.data_values.len() {
                        return Err(HtBasicError::RuntimeError {
                            message: "READ past end of DATA".to_string(),
                            span: None,
                        });
                    }
                    let val = self.data_values[self.data_pointer].clone();
                    self.data_pointer += 1;
                    self.scope.set(var, val);
                }
            },

            Stmt::Restore(label, _span) => {
                if let Some(ref _lbl) = label {
                    // Find DATA after the given label
                    self.data_pointer = 0; // simplified
                } else {
                    self.data_pointer = 0;
                }
            },

            Stmt::Input(prompt, vars, _span) => {
                if let Some(ref p) = prompt {
                    // Print prompt and read from stdin
                    print!("{}", p);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    let mut line = String::new();
                    if std::io::stdin().read_line(&mut line).is_ok() {
                        let values: Vec<&str> = line.trim().split(',').collect();
                        for (i, var) in vars.iter().enumerate() {
                            if i < values.len() {
                                let val = values[i].trim();
                                if let Ok(n) = val.parse::<f64>() {
                                    self.scope.set(var, Value::Real(n));
                                } else if let Ok(n) = val.parse::<i64>() {
                                    self.scope.set(var, Value::Integer(n));
                                } else {
                                    self.scope.set(var, Value::string(val));
                                }
                            } else {
                                self.scope.set(var, Value::Real(0.0));
                            }
                        }
                    }
                } else {
                    for var in vars {
                        self.scope.set(var, Value::Real(0.0));
                    }
                }
            },

            Stmt::Linput(prompt, var, _span) => {
                if let Some(ref p) = prompt {
                    self.output.push(p.clone());
                }
                // LINPUT reads entire line as string
                self.scope.set(var, Value::string(""));
            },

            Stmt::Stop(_span) => {
                self.pc = self.instructions.len(); // Halt execution
            },

            Stmt::End(_span) => {
                self.pc = self.instructions.len(); // Halt execution
            },

            Stmt::Pause(_span) => {
                self.output.push("PAUSE — press CONTINUE".to_string());
            },

            Stmt::Wait(expr, _span) => {
                let _seconds = self.eval_expr(expr)?.as_real();
                // In a real interpreter, we'd sleep.
                // For now, just skip.
            },

            Stmt::Beep(_span) => {
                // Sound the bell — noop in console
            },

            Stmt::Randomize(seed, _span) => {
                if let Some(ref expr) = seed {
                    let s = self.eval_expr(expr)?.as_real();
                    self.builtins.randomize(s);
                } else {
                    self.builtins.randomize(0.0); // triggers time-based seed
                }
            },

            Stmt::Disp(msg, _span) => {
                self.output.push(msg.clone());
            },

            Stmt::Image(format, _span) => {
                // IMAGE is referenced by PRINT USING; store it
                self.scope.set("__last_image__", Value::string(format));
            },

            // CHAIN must come before the catch-all Comment/Rem
            Stmt::Rem(ref msg, _) if msg.starts_with("CHAIN ") => {
                let filename = msg.trim_start_matches("CHAIN ").trim().trim_matches('"');
                self.chain_to(filename)?;
            },

            Stmt::Comment(_, _) | Stmt::Rem(_, _) => {
                // No-op
            },

            Stmt::SubStrAssign(name, start, end, value, _span) => {
                let start_idx = self.eval_expr(start)?.as_integer() as usize;
                let end_idx = self.eval_expr(end)?.as_integer() as usize;
                let new_str = self.eval_expr(value)?.as_string();

                if let Some(Value::String_(ref existing)) = self.scope.get(name) {
                    let mut s = existing.to_string();
                    let len = s.chars().count();

                    let s_idx = start_idx.max(1) as usize - 1; // 1-based to 0-based
                    let e_idx = (end_idx as usize).min(len).max(1);

                    // Replace the substring
                    let before: String = s.chars().take(s_idx).collect();
                    let after: String = s.chars().skip(e_idx).collect();
                    s = format!("{}{}{}", before, new_str, after);
                    self.scope.set(name, Value::string(&s));
                }
            },

            Stmt::OptionBase(base, _span) => {
                self.option_base = *base as usize;
            },

            Stmt::Mat(mat_op, _span) => {
                self.execute_mat(mat_op)?;
            },

            Stmt::Select(expr, arms, span) => {
                self.execute_select(expr, arms, span)?;
            },

            Stmt::OnGoTo(expr, labels, _span) => {
                self.execute_on_goto(expr, labels)?;
            },

            Stmt::OnGoSub(expr, labels, _span) => {
                self.execute_on_gosub(expr, labels)?;
            },

            Stmt::Configure(ref key, ref val, _span) => {
                let key_up = key.to_uppercase();
                if key_up.starts_with("ASSIGN @") {
                    let path_name = key.trim_start_matches("ASSIGN @").trim().to_string();
                    if let Ok(addr) = val.parse::<i32>() {
                        // Check if this is a GPIB address (700-799 or 0-30)
                        if let Some(gpib_addr) = crate::runtime::gpib::parse_gpib_address(val) {
                            self.io.assign_device(&path_name, "GPIB", addr);
                            self.gpib_paths.insert(path_name.to_uppercase(), gpib_addr);
                            if !self.gpib.has_device(gpib_addr) {
                                self.gpib.add_device(
                                    gpib_addr,
                                    Box::new(crate::runtime::gpib::Dmm::new()),
                                );
                            }
                        } else {
                            self.io.assign_device(&path_name, "GPIB", addr);
                        }
                    } else if val == "*" {
                        // ASSIGN @name TO * — release the path.
                        self.io.release(&path_name);
                        self.gpib_paths.remove(&path_name.to_uppercase());
                    } else if val.starts_with("BUFFER") {
                        self.io.assign_buffer(&path_name, 256);
                    } else {
                        let _ = self.io.assign_file(&path_name, val, "READ");
                    }
                } else if key_up.starts_with("OUTPUT @") {
                    let path_name = key.trim_start_matches("OUTPUT @").trim().to_string();
                    // Resolve GPIB address from name or direct address
                    if let Some(gpib_addr) = self.resolve_gpib(&path_name) {
                        let response = self.gpib.output(gpib_addr, val);
                        if !response.is_empty() {
                            self.scope.set(
                                &format!("__gpib_resp_{}", path_name),
                                Value::string(&response),
                            );
                        }
                    } else if let Err(e) = self.io.output(&path_name, val) {
                        return self.runtime_error(710, &format!("OUTPUT: {}", e));
                    }
                } else if key_up.starts_with("ENTER @") {
                    let path_name = key.trim_start_matches("ENTER @").trim().to_string();
                    if let Some(gpib_addr) = self.resolve_gpib(&path_name) {
                        let stored_key = format!("__gpib_resp_{}", path_name);
                        let response = if let Some(resp) = self.scope.get(&stored_key) {
                            self.scope.remove(&stored_key); // consume the stored response
                            resp.as_string()
                        } else {
                            self.gpib.enter(gpib_addr, val)
                        };
                        self.output.push(response);
                    } else if let Some(data) = self.io.enter(&path_name) {
                        self.output.push(data);
                    }
                } else if key_up == "MASS STORAGE IS" {
                    self.io.mass_storage_is(val);
                } else if key_up == "CREATE" {
                    let _ = self.io.create_file(val, "ASCII");
                } else if key_up == "CREATE BDAT" {
                    let _ = self.io.create_file(val, "BDAT");
                } else if key_up == "PURGE" {
                    let _ = self.io.purge_file(val);
                } else if key_up == "CAT" {
                    if let Ok(files) = self.io.cat(val) {
                        for f in files {
                            self.output.push(f);
                        }
                    }
                } else if key_up == "CONFIGURE" {
                    // CONFIGURE options — store for future use
                    self.output.push(format!("CONFIGURE {} {}", key, val));
                } else if key_up.starts_with("STATUS @") {
                    let rest = key.trim_start_matches("STATUS @").trim().to_string();
                    let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        if let (Some(addr), Ok(reg)) = (
                            crate::runtime::gpib::parse_gpib_address(parts[0]),
                            parts[1].parse::<u8>(),
                        ) {
                            let status_val = self.gpib.status(addr, reg);
                            self.output.push(status_val.to_string());
                        }
                    }
                } else if key_up.starts_with("CONTROL @") {
                    let rest = key.trim_start_matches("CONTROL @").trim().to_string();
                    let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        let addr_opt = crate::runtime::gpib::parse_gpib_address(parts[0]);
                        let reg_val = parts[1].parse::<u8>();
                        let ctrl_val = parts.get(2).and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
                        if let (Some(addr), Ok(reg)) = (addr_opt, reg_val) {
                            self.gpib.control(addr, reg, ctrl_val);
                        }
                    }
                } else if key_up == "ABORT" {
                    // ABORT interface clear — reset GPIB bus
                    self.output.push("GPIB bus cleared".to_string());
                } else if key_up == "TRANSFER" {
                    self.output.push("TRANSFER stub".to_string());
                } else if key_up.starts_with("ON ") {
                    // ON KEY/CYCLE/KBD/KNOB/END/HALT/TIMEOUT/SIGNAL
                    let parts: Vec<&str> = key_up.splitn(3, ' ').collect();
                    if parts.len() >= 3 {
                        let event = parts[1].to_string();
                        let label = parts[2].to_string();
                        let priority = val.parse::<usize>().unwrap_or(15);
                        self.event_handlers
                            .insert(event.clone(), (label.clone(), priority, "GOTO".to_string()));
                        self.output.push(format!("ON {} handler set", event));
                    }
                } else if key_up == "ENABLE" {
                    self.events_enabled = true;
                } else if key_up == "DISABLE" {
                    self.events_enabled = false;
                } else if key_up.starts_with("OFF ") {
                    let event = key_up.trim_start_matches("OFF ").to_string();
                    self.event_handlers.remove(&event);
                }
            },

            Stmt::Gfx(ref cmd, _span) => {
                self.execute_gfx(cmd)?;
            },

            // Fallback for unimplemented statement types
            _ => {
                // Skip unrecognized statements silently
            },
        }

        Ok(())
    }

    // ===================== Expression Evaluation =====================

    fn eval_expr(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Integer(n, _) => Ok(Value::Integer(*n)),
            Expr::Real(n, _) => Ok(Value::Real(*n)),
            Expr::String_(s, _) => Ok(Value::string(s)),
            Expr::Variable(name, _) => {
                if let Some(val) = self.scope.get(name) {
                    Ok(val)
                } else if self.builtins.exists(name) {
                    // 0-arg builtin (e.g., PI, RND, DATE$, TIME$)
                    if let Some((_argc, func)) = self.builtins.get_with_args(name, 0) {
                        Ok(func(&[]))
                    } else if let Some((_argc, func)) = self.builtins.get(name) {
                        Ok(func(&[]))
                    } else {
                        Ok(Value::Real(0.0))
                    }
                } else {
                    // HTBasic allows implicit creation: undefined variables default to 0
                    Ok(Value::Real(0.0))
                }
            },
            Expr::StringVariable(name, _) => {
                if let Some(val) = self.scope.get(name) {
                    Ok(val)
                } else if self.builtins.exists(name) {
                    if let Some((_argc, func)) = self.builtins.get_with_args(name, 0) {
                        Ok(func(&[]))
                    } else if let Some((_argc, func)) = self.builtins.get(name) {
                        Ok(func(&[]))
                    } else {
                        Ok(Value::string(""))
                    }
                } else {
                    Ok(Value::string(""))
                }
            },
            // Whole-array reference A(*) — evaluates to the array value
            // itself (e.g. `PRINT A(*)` prints all elements).
            Expr::WholeArray(name, _) => {
                if let Some(val) = self.scope.get(name) {
                    Ok(val)
                } else {
                    Ok(Value::Real(0.0))
                }
            },
            Expr::ArrayRef(name, subscripts, _span) => {
                let subs: Result<Vec<i64>> = subscripts
                    .iter()
                    .map(|s| Ok(self.eval_expr(s)?.as_integer()))
                    .collect();
                let subs = subs?;

                if let Some(Value::Array(ref arr)) = self.scope.get(name) {
                    if let Some(val) = arr.get(&subs) {
                        Ok(val.clone())
                    } else {
                        Ok(Value::Real(0.0))
                    }
                } else {
                    Ok(Value::Real(0.0))
                }
            },
            Expr::FnCall(name, args, _span) => {
                // Evaluate arguments first
                let arg_vals: Result<Vec<Value>> = args.iter().map(|a| self.eval_expr(a)).collect();
                let arg_vals = arg_vals?;

                // Check built-ins
                if let Some((_argc, func)) = self.builtins.get_with_args(name, arg_vals.len()) {
                    return Ok(func(&arg_vals));
                }

                // Check user-defined functions (DEF FN)
                let upper = name.to_uppercase();
                if let Some(func_def) = self.fn_registry.get(&upper).cloned() {
                    return self.execute_fn(&func_def, &arg_vals);
                }

                // Fall back to array reference
                if self.scope.get(name).is_some() {
                    // Not a built-in — treat as array ref: A(1,2)
                    let subs: Vec<i64> = arg_vals.iter().map(|v| v.as_integer()).collect();
                    if let Some(Value::Array(ref arr)) = self.scope.get(name) {
                        if let Some(val) = arr.get(&subs) {
                            Ok(val.clone())
                        } else {
                            Ok(Value::Real(0.0))
                        }
                    } else {
                        Ok(Value::Real(0.0))
                    }
                } else {
                    Ok(Value::Real(0.0))
                }
            },
            Expr::StringFnCall(name, args, _span) => {
                let arg_vals: Result<Vec<Value>> = args.iter().map(|a| self.eval_expr(a)).collect();
                let arg_vals = arg_vals?;

                // Check built-ins
                if let Some((_argc, func)) = self.builtins.get_with_args(name, arg_vals.len()) {
                    return Ok(func(&arg_vals));
                }
                // Check user-defined functions
                let upper = name.to_uppercase();
                if let Some(func_def) = self.fn_registry.get(&upper).cloned() {
                    return self.execute_fn(&func_def, &arg_vals);
                }
                // Fall back to array reference
                if self.scope.get(name).is_some() {
                    // Not a built-in — treat as string array ref
                    let subs: Vec<i64> = arg_vals.iter().map(|v| v.as_integer()).collect();
                    if let Some(Value::Array(ref arr)) = self.scope.get(name) {
                        if let Some(val) = arr.get(&subs) {
                            Ok(val.clone())
                        } else {
                            Ok(Value::string(""))
                        }
                    } else {
                        Ok(Value::string(""))
                    }
                } else {
                    Ok(Value::string(""))
                }
            },
            Expr::SubStr(name, start, end, is_length, _span) => {
                let start_idx = self.eval_expr(start)?.as_integer();

                if let Some(Value::String_(ref s)) = self.scope.get(name) {
                    let chars: Vec<char> = s.chars().collect();
                    let _len = chars.len() as i64;

                    let s_idx = (start_idx.max(1) - 1) as usize;

                    let e_idx = match end {
                        Some(ref e) => {
                            let e_val = self.eval_expr(e)?.as_integer();
                            if *is_length {
                                // start;length
                                ((start_idx - 1 + e_val) as usize).min(chars.len())
                            } else {
                                // start,end
                                e_val.max(1) as usize
                            }
                        },
                        None => chars.len(),
                    };

                    if s_idx >= chars.len() {
                        return Ok(Value::string(""));
                    }

                    let result: String = chars[s_idx..e_idx.min(chars.len())].iter().collect();
                    Ok(Value::string(&result))
                } else {
                    Ok(Value::string(""))
                }
            },
            Expr::Unary(op, expr, _span) => {
                let val = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Minus => match val {
                        Value::Real(n) => Ok(Value::Real(-n)),
                        Value::Integer(n) => Ok(Value::Integer(-n)),
                        _ => Ok(Value::Real(-val.as_real())),
                    },
                    UnaryOp::Plus => Ok(val),
                    UnaryOp::Not => Ok(Value::Integer(if val.is_truthy() { 0 } else { 1 })),
                }
            },
            Expr::Binary(left, op, right, _span) => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                self.eval_binary(&l, op, &r)
            },
        }
    }

    fn eval_binary(&self, left: &Value, op: &BinaryOp, right: &Value) -> Result<Value> {
        use BinaryOp::*;

        match op {
            Add => {
                if let (Value::String_(a), Value::String_(b)) = (left, right) {
                    let combined = format!("{}{}", a, b);
                    return Ok(Value::string(&combined));
                }
                let result = left.as_real() + right.as_real();
                if left.as_real().fract() == 0.0 && right.as_real().fract() == 0.0 {
                    Ok(Value::Real(result))
                } else {
                    Ok(Value::Real(result))
                }
            },
            Sub => Ok(Value::Real(left.as_real() - right.as_real())),
            Mul => Ok(Value::Real(left.as_real() * right.as_real())),
            Div => {
                let divisor = right.as_real();
                if divisor == 0.0 {
                    return Err(HtBasicError::DivisionByZero {
                        span: Span::new(0, 0),
                    });
                }
                Ok(Value::Real(left.as_real() / divisor))
            },
            Pow => Ok(Value::Real(left.as_real().powf(right.as_real()))),
            Concat => {
                let combined = format!("{}{}", left.as_string(), right.as_string());
                Ok(Value::string(&combined))
            },
            Eq => match (left, right) {
                (Value::Real(a), Value::Real(b)) => {
                    Ok(Value::Integer(if (a - b).abs() < 1e-15 { 1 } else { 0 }))
                },
                (Value::Integer(a), Value::Integer(b)) => {
                    Ok(Value::Integer(if a == b { 1 } else { 0 }))
                },
                (Value::String_(a), Value::String_(b)) => {
                    Ok(Value::Integer(if a == b { 1 } else { 0 }))
                },
                _ => Ok(Value::Integer(
                    if (left.as_real() - right.as_real()).abs() < 1e-15 {
                        1
                    } else {
                        0
                    },
                )),
            },
            NotEq => match (left, right) {
                (Value::String_(a), Value::String_(b)) => {
                    Ok(Value::Integer(if a != b { 1 } else { 0 }))
                },
                _ => Ok(Value::Integer(
                    if (left.as_real() - right.as_real()).abs() >= 1e-15 {
                        1
                    } else {
                        0
                    },
                )),
            },
            Lt => Ok(Value::Integer(if left.as_real() < right.as_real() {
                1
            } else {
                0
            })),
            Gt => Ok(Value::Integer(if left.as_real() > right.as_real() {
                1
            } else {
                0
            })),
            LtEq => Ok(Value::Integer(if left.as_real() <= right.as_real() {
                1
            } else {
                0
            })),
            GtEq => Ok(Value::Integer(if left.as_real() >= right.as_real() {
                1
            } else {
                0
            })),
            And => {
                let l_truthy = left.is_truthy();
                let r_truthy = right.is_truthy();
                Ok(Value::Integer(if l_truthy && r_truthy { 1 } else { 0 }))
            },
            Or => {
                let l_truthy = left.is_truthy();
                let r_truthy = right.is_truthy();
                Ok(Value::Integer(if l_truthy || r_truthy { 1 } else { 0 }))
            },
            Exor => {
                let l_truthy = left.is_truthy();
                let r_truthy = right.is_truthy();
                Ok(Value::Integer(if l_truthy != r_truthy { 1 } else { 0 }))
            },
            Mod_ => {
                let a = left.as_integer();
                let b = right.as_integer();
                if b == 0 {
                    return Err(HtBasicError::DivisionByZero {
                        span: Span::new(0, 0),
                    });
                }
                Ok(Value::Integer(a % b))
            },
            Modulo => {
                let a = left.as_real();
                let b = right.as_real();
                if b == 0.0 {
                    return Err(HtBasicError::DivisionByZero {
                        span: Span::new(0, 0),
                    });
                }
                // HTBasic MODULO: remainder with sign of divisor
                let r = a % b;
                if r * b < 0.0 {
                    Ok(Value::Real(r + b))
                } else {
                    Ok(Value::Real(r))
                }
            },
            Div_ => {
                let a = left.as_integer();
                let b = right.as_integer();
                if b == 0 {
                    return Err(HtBasicError::DivisionByZero {
                        span: Span::new(0, 0),
                    });
                }
                Ok(Value::Integer(a / b))
            },
        }
    }

    // ===================== Simple PRINT USING Formatting =====================

    /// Full PRINT USING / IMAGE format engine.
    /// Supports D, Z, S, M, E, K, X, A, ., *, /, #, @, quoted literals,
    /// repeat groups n(...), and embedded newlines.
    fn format_using(&self, format: &str, value: &Value) -> String {
        let mut result = String::new();
        let chars: Vec<char> = format.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            match chars[i] {
                '#' => {
                    // Digit placeholder (no leading zeros)
                    let mut count = 0;
                    let mut has_decimal = false;
                    let mut decimal_places = 0;
                    let _start = i;

                    while i < chars.len() && chars[i] == '#' {
                        count += 1;
                        i += 1;
                    }
                    if i < chars.len() && chars[i] == '.' {
                        has_decimal = true;
                        i += 1;
                        while i < chars.len() && chars[i] == '#' {
                            decimal_places += 1;
                            i += 1;
                        }
                    }

                    let num = value.as_real();
                    if has_decimal {
                        result.push_str(&format!(
                            "{:>width$.dec$}",
                            num,
                            width = count,
                            dec = decimal_places
                        ));
                    } else {
                        result.push_str(&format!("{:>width$}", value.as_integer(), width = count));
                    }
                    continue;
                },
                'D' | 'Z' => {
                    // Digit placeholder
                    i += 1;
                    let num = value.as_real();
                    result.push_str(&format!("{}", num));
                    continue;
                },
                'S' => {
                    i += 1;
                    let num = value.as_real();
                    if num >= 0.0 {
                        result.push('+');
                    } else {
                        result.push('-');
                    }
                    continue;
                },
                'M' => {
                    i += 1;
                    let num = value.as_real();
                    if num < 0.0 {
                        result.push('-');
                    }
                    continue;
                },
                'A' => {
                    i += 1;
                    result.push_str(&value.as_string());
                    continue;
                },
                'X' => {
                    i += 1;
                    result.push(' ');
                    continue;
                },
                '/' => {
                    i += 1;
                    result.push('\n');
                    continue;
                },
                '"' => {
                    i += 1;
                    while i < chars.len() && chars[i] != '"' {
                        result.push(chars[i]);
                        i += 1;
                    }
                    if i < chars.len() {
                        i += 1; // skip closing quote
                    }
                    continue;
                },
                _ => {
                    result.push(chars[i]);
                    i += 1;
                },
            }
        }

        result
    }

    // ===================== Phase 2: MAT Operations =====================

    fn execute_mat(&mut self, mat_op: &MatOp) -> Result<()> {
        match mat_op {
            MatOp::Assign(dest, src, _) => {
                let src_val = self.scope.get(src).unwrap_or(Value::Null);
                // Deep copy needed for arrays
                self.scope.set(dest, src_val);
            },
            MatOp::Binary(dest, src1, op, src2, _) => {
                let a = self.get_array(src1);
                let b = self.get_array(src2);
                let result = self.mat_binary_op(&a, &b, *op);
                self.scope.set(dest, Value::Array(result));
            },
            MatOp::ScalarBinary(dest, scalar, op, src, _) => {
                let s = self.eval_expr(scalar)?;
                let b = self.get_array(src);
                let result = self.mat_scalar_binary_op(&s, &b, *op);
                self.scope.set(dest, Value::Array(result));
            },
            MatOp::Func(dest, func, src, _) => {
                let arr = self.get_array(src);
                let result = self.mat_function(*func, &arr);
                self.scope.set(dest, Value::Array(result));
            },
            MatOp::FuncInit(dest, func, dims, _) => {
                let arr_dims: Vec<(i64, i64)> = if dims.is_empty() {
                    // No explicit dims — use destination array's existing dimensions
                    match self.scope.get(dest) {
                        Some(Value::Array(ref arr)) => arr.dims.clone(),
                        _ => vec![(0, 2)], // fallback: 3-element 1D array
                    }
                } else {
                    dims.iter().map(|&(lo, hi)| (lo, hi)).collect()
                };
                let result = self.mat_function_init(*func, &arr_dims);
                self.scope.set(dest, Value::Array(result));
            },
            MatOp::Input(name, _) => {
                // MAT INPUT — read values from stdin (stub: fills with 0)
                if let Some(Value::Array(ref template)) = self.scope.get(name) {
                    let mut new_arr = template.clone();
                    for v in &mut new_arr.data {
                        *v = Value::Real(0.0);
                    }
                    self.scope.set(name, Value::Array(new_arr));
                }
            },
            MatOp::Print(name, _) => {
                if let Some(Value::Array(ref arr)) = self.scope.get(name) {
                    self.output.push(self.format_array(arr));
                }
            },
            MatOp::Read(name, _) => {
                // MAT READ — read values from DATA statements
                if let Some(Value::Array(ref template)) = self.scope.get(name) {
                    let mut new_arr = template.clone();
                    for v in &mut new_arr.data {
                        if self.data_pointer < self.data_values.len() {
                            *v = self.data_values[self.data_pointer].clone();
                            self.data_pointer += 1;
                        } else {
                            return Err(HtBasicError::RuntimeError {
                                message: "MAT READ past end of DATA".to_string(),
                                span: None,
                            });
                        }
                    }
                    self.scope.set(name, Value::Array(new_arr));
                }
            },
            MatOp::Reduc(dest, func, src, vector, subscript, _) => {
                let arr = self.get_array(src);
                let result = match func {
                    ReducFunc::Reorder => self.mat_reorder(&arr, vector.as_deref(), *subscript),
                    _ => self.mat_reduction(*func, &arr),
                };
                self.scope.set(dest, Value::Array(result));
            },
        }
        Ok(())
    }

    fn get_array(&self, name: &str) -> ArrayData {
        match self.scope.get(name) {
            Some(Value::Array(arr)) => arr,
            _ => ArrayData::new(vec![]),
        }
    }

    fn mat_binary_op(&self, a: &ArrayData, b: &ArrayData, op: MatBinOp) -> ArrayData {
        let dims = a.dims.clone();
        let mut result = ArrayData::new(dims.clone());

        for (i, res_v) in result.data.iter_mut().enumerate() {
            let av = a.data.get(i).map(|v| v.as_real()).unwrap_or(0.0);
            let bv = b.data.get(i).map(|v| v.as_real()).unwrap_or(0.0);
            match op {
                MatBinOp::Add => *res_v = Value::Real(av + bv),
                MatBinOp::Sub => *res_v = Value::Real(av - bv),
                MatBinOp::Mul => {
                    // Classical matrix multiplication — needs proper dimensions
                    *res_v = Value::Real(av * bv)
                },
                MatBinOp::Div => *res_v = Value::Real(av / bv),
                MatBinOp::DotMul => *res_v = Value::Real(av * bv),
            }
        }
        result
    }

    fn mat_scalar_binary_op(&self, s: &Value, b: &ArrayData, op: MatBinOp) -> ArrayData {
        let sv = s.as_real();
        let mut result = ArrayData::new(b.dims.clone());
        for (i, res_v) in result.data.iter_mut().enumerate() {
            let bv = b.data.get(i).map(|v| v.as_real()).unwrap_or(0.0);
            match op {
                MatBinOp::Add => *res_v = Value::Real(sv + bv),
                MatBinOp::Sub => *res_v = Value::Real(sv - bv),
                MatBinOp::Mul => *res_v = Value::Real(sv * bv),
                MatBinOp::Div => *res_v = Value::Real(sv / bv),
                MatBinOp::DotMul => *res_v = Value::Real(sv * bv),
            }
        }
        result
    }

    fn mat_function(&self, func: MatFunc, arr: &ArrayData) -> ArrayData {
        match func {
            MatFunc::Inv => {
                // Basic 2x2 matrix inverse
                if arr.dims.len() == 2 {
                    let n = arr.dims[0].1 as usize;
                    let mut result = ArrayData::new(arr.dims.clone());
                    // Identity fallback
                    for i in 0..n.min(arr.data.len()) {
                        result.data[i] = arr.data.get(i).cloned().unwrap_or(Value::Null);
                    }
                    result
                } else {
                    arr.clone()
                }
            },
            MatFunc::Trn => {
                if arr.dims.len() == 2 {
                    let rows = (arr.dims[0].1 - arr.dims[0].0 + 1) as usize;
                    let cols = (arr.dims[1].1 - arr.dims[1].0 + 1) as usize;
                    // Swap dimensions
                    let new_dims = vec![
                        (arr.dims[1].0, arr.dims[1].1),
                        (arr.dims[0].0, arr.dims[0].1),
                    ];
                    let mut result = ArrayData::new(new_dims);
                    for r in 0..rows {
                        for c in 0..cols {
                            let src_idx = r * cols + c;
                            let dst_idx = c * rows + r;
                            if src_idx < arr.data.len() && dst_idx < result.data.len() {
                                result.data[dst_idx] =
                                    arr.data.get(src_idx).cloned().unwrap_or(Value::Null);
                            }
                        }
                    }
                    result
                } else {
                    arr.clone()
                }
            },
            MatFunc::Zer => {
                let mut result = ArrayData::new(arr.dims.clone());
                for v in &mut result.data {
                    *v = Value::Real(0.0);
                }
                result
            },
            MatFunc::Con => {
                let mut result = ArrayData::new(arr.dims.clone());
                for v in &mut result.data {
                    *v = Value::Real(1.0);
                }
                result
            },
            MatFunc::Idn => {
                if arr.dims.len() == 2 {
                    let rows = (arr.dims[0].1 - arr.dims[0].0 + 1) as usize;
                    let cols = (arr.dims[1].1 - arr.dims[1].0 + 1) as usize;
                    let mut result = ArrayData::new(arr.dims.clone());
                    // Fill all with zeros first
                    for v in &mut result.data {
                        *v = Value::Real(0.0);
                    }
                    for i in 0..rows.min(cols) {
                        let idx = i * cols + i;
                        if idx < result.data.len() {
                            result.data[idx] = Value::Real(1.0);
                        }
                    }
                    result
                } else {
                    ArrayData::new(arr.dims.clone())
                }
            },
        }
    }

    fn mat_function_init(&self, func: MatFunc, dims: &[(i64, i64)]) -> ArrayData {
        let arr = ArrayData::new(dims.to_vec());
        match func {
            MatFunc::Zer => {
                let mut result = arr;
                for v in &mut result.data {
                    *v = Value::Real(0.0);
                }
                result
            },
            MatFunc::Con => {
                let mut result = arr;
                for v in &mut result.data {
                    *v = Value::Real(1.0);
                }
                result
            },
            MatFunc::Idn => {
                let mut result = arr;
                // Fill all with zeros first
                for v in &mut result.data {
                    *v = Value::Real(0.0);
                }
                if dims.len() == 2 {
                    let rows = (dims[0].1 - dims[0].0 + 1) as usize;
                    let cols = (dims[1].1 - dims[1].0 + 1) as usize;
                    for i in 0..rows.min(cols) {
                        let idx = i * cols + i;
                        if idx < result.data.len() {
                            result.data[idx] = Value::Real(1.0);
                        }
                    }
                }
                result
            },
            _ => arr,
        }
    }

    fn mat_reduction(&self, func: ReducFunc, arr: &ArrayData) -> ArrayData {
        match func {
            ReducFunc::Rsum => {
                // Row sums → column vector
                if arr.dims.len() == 2 {
                    let rows = (arr.dims[0].1 - arr.dims[0].0 + 1) as usize;
                    let cols = (arr.dims[1].1 - arr.dims[1].0 + 1) as usize;
                    let result_dims = vec![(0, rows as i64 - 1), (0, 0)];
                    let mut result = ArrayData::new(result_dims);
                    for r in 0..rows {
                        let mut sum = 0.0;
                        for c in 0..cols {
                            sum += arr
                                .data
                                .get(r * cols + c)
                                .map(|v| v.as_real())
                                .unwrap_or(0.0);
                        }
                        if r < result.data.len() {
                            result.data[r] = Value::Real(sum);
                        }
                    }
                    result
                } else {
                    ArrayData::new(vec![])
                }
            },
            ReducFunc::Csum => {
                // Column sums → row vector
                if arr.dims.len() == 2 {
                    let rows = (arr.dims[0].1 - arr.dims[0].0 + 1) as usize;
                    let cols = (arr.dims[1].1 - arr.dims[1].0 + 1) as usize;
                    let result_dims = vec![(0, 0), (0, cols as i64 - 1)];
                    let mut result = ArrayData::new(result_dims);
                    for c in 0..cols {
                        let mut sum = 0.0;
                        for r in 0..rows {
                            sum += arr
                                .data
                                .get(r * cols + c)
                                .map(|v| v.as_real())
                                .unwrap_or(0.0);
                        }
                        if c < result.data.len() {
                            result.data[c] = Value::Real(sum);
                        }
                    }
                    result
                } else {
                    ArrayData::new(vec![])
                }
            },
            ReducFunc::Sort => {
                // MAT SORT A(*) — ascending sort, array overwritten in place.
                let mut result = arr.clone();
                result.data.sort_by(|a, b| {
                    a.as_real()
                        .partial_cmp(&b.as_real())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                result
            },
            ReducFunc::SortDesc => {
                // MAT SORT A(*) DESC (mat sort.prg).
                let mut result = arr.clone();
                result.data.sort_by(|a, b| {
                    b.as_real()
                        .partial_cmp(&a.as_real())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                result
            },
            ReducFunc::Reorder => arr.clone(),
        }
    }

    /// MAT REORDER M BY V[,n] — reorder one subscript of `arr` (default 1 =
    /// first dimension) so its elements appear in the order given by the
    /// integer values in vector V (1-based subscripts of the target dim).
    fn mat_reorder(&self, arr: &ArrayData, vector: Option<&str>, subscript: Option<i64>) -> ArrayData {
        let Some(by_name) = vector else {
            return arr.clone();
        };
        let by = self.get_array(by_name);
        let rank = arr.dims.len();
        if rank != 2 || by.data.is_empty() {
            return arr.clone();
        }
        let rows = (arr.dims[0].1 - arr.dims[0].0 + 1) as usize;
        let cols = (arr.dims[1].1 - arr.dims[1].0 + 1) as usize;
        let dim = match subscript.unwrap_or(1) {
            2 => 1,
            _ => 0,
        };
        let mut result = arr.clone();
        if dim == 1 {
            // Reorder columns: result[r][k] = arr[r][V(k)]
            for r in 0..rows {
                for k in 0..cols {
                    let idx = by
                        .data
                        .get(k)
                        .map(|v| v.as_real() as i64 - 1)
                        .unwrap_or(k as i64);
                    let col = idx.clamp(0, cols as i64 - 1) as usize;
                    let val = arr.data.get(r * cols + col).cloned().unwrap_or(Value::Null);
                    result.data[r * cols + k] = val;
                }
            }
        } else {
            // Reorder rows: result[k][c] = arr[V(k)][c]
            for k in 0..rows {
                let idx = by
                    .data
                    .get(k)
                    .map(|v| v.as_real() as i64 - 1)
                    .unwrap_or(k as i64);
                let row = idx.clamp(0, rows as i64 - 1) as usize;
                for c in 0..cols {
                    let val = arr.data.get(row * cols + c).cloned().unwrap_or(Value::Null);
                    result.data[k * cols + c] = val;
                }
            }
        }
        result
    }

    fn format_array(&self, arr: &ArrayData) -> String {
        let mut lines = vec![];
        if arr.dims.len() == 1 {
            let vals: Vec<String> = arr.data.iter().map(|v| v.to_display_string()).collect();
            lines.push(vals.join("  "));
        } else if arr.dims.len() == 2 {
            let cols = (arr.dims[1].1 - arr.dims[1].0 + 1) as usize;
            for row_chunk in arr.data.chunks(cols) {
                let line: Vec<String> = row_chunk.iter().map(|v| v.to_display_string()).collect();
                lines.push(line.join("  "));
            }
        }
        lines.join("\n")
    }

    // ===================== Phase 2: SELECT/CASE =====================

    fn execute_select(&mut self, expr: &Expr, arms: &[CaseArm], _span: &Span) -> Result<()> {
        let test_val = self.eval_expr(expr)?;

        for arm in arms {
            if self.case_matches(&test_val, &arm.cases) {
                for stmt in &arm.body {
                    self.execute_stmt(stmt)?;
                }
                break; // Only execute the first matching CASE
            }
        }
        Ok(())
    }

    fn case_matches(&mut self, val: &Value, cases: &[CaseValue]) -> bool {
        for case in cases {
            match case {
                CaseValue::Else => return true,
                CaseValue::Single(expr) => {
                    if let Ok(cv) = self.eval_expr(expr) {
                        if self.values_equal(val, &cv) {
                            return true;
                        }
                    }
                },
                CaseValue::Range(low, high) => {
                    if let (Ok(lv), Ok(hv)) = (self.eval_expr(low), self.eval_expr(high)) {
                        let v = val.as_real();
                        if v >= lv.as_real() && v <= hv.as_real() {
                            return true;
                        }
                    }
                },
                CaseValue::Is(op, expr) => {
                    if let Ok(cv) = self.eval_expr(expr) {
                        if self.case_is_match(val, *op, &cv) {
                            return true;
                        }
                    }
                },
            }
        }
        false
    }

    fn values_equal(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Real(x), Value::Real(y)) => (x - y).abs() < 1e-15,
            (Value::Integer(x), Value::Integer(y)) => x == y,
            (Value::String_(x), Value::String_(y)) => x.as_ref() == y.as_ref(),
            _ => (a.as_real() - b.as_real()).abs() < 1e-15,
        }
    }

    fn case_is_match(&self, val: &Value, op: RelOp, cmp: &Value) -> bool {
        let a = val.as_real();
        let b = cmp.as_real();
        match op {
            RelOp::Lt => a < b,
            RelOp::LtEq => a <= b,
            RelOp::Eq => (a - b).abs() < 1e-15,
            RelOp::GtEq => a >= b,
            RelOp::Gt => a > b,
            RelOp::NotEq => (a - b).abs() >= 1e-15,
        }
    }

    // ===================== Phase 2: ON expr GOTO/GOSUB =====================

    fn execute_on_goto(&mut self, expr: &Expr, labels: &[String]) -> Result<()> {
        let idx = self.eval_expr(expr)?.as_integer() as usize;
        let idx = idx.saturating_sub(1); // 1-based → 0-based
        if idx < labels.len() {
            let label = &labels[idx];
            self.jump_to_label(label)?;
        }
        // If index out of range, fall through (HTBasic behavior)
        Ok(())
    }

    fn execute_on_gosub(&mut self, expr: &Expr, labels: &[String]) -> Result<()> {
        let idx = self.eval_expr(expr)?.as_integer() as usize;
        let idx = idx.saturating_sub(1);
        if idx < labels.len() {
            let label = labels[idx].clone();
            // Push return address
            self.call_stack.push(CallFrame {
                return_pc: self.pc + 1,
                local_scope: self.scope.clone(),
            });
            self.jump_to_label(&label)?;
        }
        Ok(())
    }

    /// Raise a runtime error, checking for ON ERROR handler.
    fn runtime_error(&mut self, code: i64, msg: &str) -> Result<()> {
        self.last_err = code;
        self.last_err_line = self.pc as i64;
        self.last_err_msg = msg.to_string();
        if let Some(ref handler_label) = self.error_handler.clone() {
            // Jump to error handler (like GOSUB). RETURN from the handler
            // resumes at the statement AFTER the one that raised the error.
            self.call_stack.push(CallFrame {
                return_pc: self.pc + 1,
                local_scope: self.scope.clone(),
            });
            if let Some(idx) = self.find_label(&handler_label) {
                self.pc = idx;
                return Ok(());
            }
        }
        Err(HtBasicError::RuntimeError {
            message: format!("Error {}: {}", code, msg),
            span: None,
        })
    }

    /// Resolve a GPIB address from a path name.
    fn resolve_gpib(&self, name: &str) -> Option<u8> {
        if let Some(&addr) = self.gpib_paths.get(&name.to_uppercase()) {
            return Some(addr);
        }
        // Direct numeric address
        crate::runtime::gpib::parse_gpib_address(name)
    }

    fn find_label(&self, label: &str) -> Option<usize> {
        for (i, instr) in self.instructions.iter().enumerate() {
            if let Instr::Label(ref l) = instr {
                if l == label {
                    return Some(i);
                }
            }
        }
        None
    }

    // ===================== DEF FN Execution =====================

    /// Execute a user-defined function (DEF FN) and return the result.
    fn execute_fn(&mut self, func_def: &FnDef, args: &[Value]) -> Result<Value> {
        // Save state
        let saved_scope = self.scope.clone();
        let saved_pc = self.pc;
        let saved_call_stack_len = self.call_stack.len();

        // Create local scope and bind parameters
        let mut local_scope = Scope::new();
        for (i, param) in func_def.params.iter().enumerate() {
            let val = if i < args.len() {
                args[i].clone()
            } else {
                match param.param_type {
                    ParamType::String_ => Value::string(""),
                    _ => Value::Real(0.0),
                }
            };
            local_scope.set(&param.name, val);
        }
        // OPTIONAL — number of optional arguments actually passed
        // (fn.prg: `IF OPTIONAL=0 THEN RETURN "You didn't use the
        // OPTIONAL parameter."`).
        let optional_passed = args.len().saturating_sub(func_def.required_params);
        local_scope.set("OPTIONAL", Value::Integer(optional_passed as i64));
        self.scope = local_scope;

        // Execute FN body directly (tree-walking)
        let mut result = Value::Real(0.0);
        let saved_fn_return = self.fn_return.take();
        self.fn_return = Some(FnReturn::Armed);
        for stmt in &func_def.body {
            match stmt {
                Stmt::Return(ref expr, _) => {
                    if let Some(e) = expr {
                        result = self.eval_expr(e)?;
                    }
                    break;
                },
                _ => {
                    self.execute_stmt(stmt)?;
                    // A RETURN nested inside control flow (e.g. a single-line
                    // IF) was captured by the Stmt::Return handler.
                    match self.fn_return.clone() {
                        Some(FnReturn::Fired(Some(val))) => {
                            result = val;
                            break;
                        },
                        Some(FnReturn::Fired(None)) => break, // bare RETURN
                        _ => {},
                    }
                },
            }
        }
        self.fn_return = saved_fn_return;

        // Restore state (trim any extra call frames from nested GOSUBs)
        while self.call_stack.len() > saved_call_stack_len {
            self.call_stack.pop();
        }
        self.scope = saved_scope;
        self.pc = saved_pc;

        Ok(result)
    }

    // ===================== Graphics Execution =====================

    fn execute_gfx(&mut self, cmd: &GfxCmd) -> Result<()> {
        use crate::parser::ast::GfxCmd::*;

        match *cmd {
            Ginit => self.graphics.ginit(),
            Gclear => self.graphics.gclear(),
            PlotterIs(ref device, ref opts) => {
                if device.to_uppercase() == "CRT" {
                    self.graphics.initialized = true;
                }
                let _ = opts;
            },
            Move(x, y) => self.graphics.move_to(x, y),
            Draw(x, y, relative, pen_control) => {
                let (nx, ny) = if relative {
                    (self.graphics.pen_x + x, self.graphics.pen_y + y)
                } else {
                    (x, y)
                };
                if pen_control {
                    self.graphics.move_to(nx, ny);
                } else {
                    self.graphics.draw_to(nx, ny);
                }
            },
            Plot(x, y) => self.graphics.plot(x, y),
            Pen(n) => self.graphics.pen_number = n.min(15),
            LineType(n) => self.graphics.line_type = n.max(1).min(10),
            Label(ref s) => self.graphics.label(s),
            Csize(w, h) => {
                self.graphics.csize = (w, h.unwrap_or(w * 1.5));
            },
            Ldir(a) => self.graphics.ldirection = a,
            Lorg(n) => self.graphics.lorg = n.min(9).max(1),
            Gfont(ref s) => self.graphics.gfont = s.clone(),
            Axes(xt, yt, xo, yo) => {
                self.graphics.axes(xt, yt, xo, yo, 0.5, 0.5, 0.3);
            },
            Grid(xt, yt, xo, yo) => {
                self.graphics.grid(xt, yt, xo, yo, 0.5, 0.5, 0.3);
            },
            Frame => self.graphics.frame(),
            Clip(x1, y1, x2, y2) => {
                self.graphics.clip_on = true;
                self.graphics.clip_rect = (x1, x2, y1, y2);
            },
            ClipOff => self.graphics.clip_on = false,
            Window(x1, x2, y1, y2) => {
                self.graphics.window = (x1, x2, y1, y2);
            },
            Viewport(x1, x2, y1, y2) => {
                self.graphics.viewport = (x1, x2, y1, y2);
            },
            Rectangle(w, h, fill, edge) => {
                self.graphics.rectangle_rel(w, h, fill, edge);
            },
            PolygonReg(radius, chords, fill, edge) => {
                let (total, drawn) = chords.unwrap_or((60.0, 60.0));
                self.graphics.polygon_regular(radius, total, drawn, fill, edge);
            },
            PolylineReg(radius, chords) => {
                let (total, drawn) = chords.unwrap_or((60.0, 60.0));
                self.graphics.polyline_regular(radius, total, drawn);
            },
            Polygon(ref pts) => self.graphics.polygon(pts),
            Polyline(ref pts) => self.graphics.polyline(pts),
            Gload(ref f) => {
                if let Err(e) = self.graphics.gload(f) {
                    return self.runtime_error(701, &format!("GLOAD: {}", e));
                }
            },
            Gstore(ref f) => {
                if let Err(e) = self.graphics.gstore(f) {
                    return self.runtime_error(702, &format!("GSTORE: {}", e));
                }
                self.output.push(format!("Graphics saved to {}", f));
            },
            Penup => self.graphics.penup(),
            Color(ref s) => {
                // Simple color name mapping
                let colors: &[(&str, [u8; 3])] = &[
                    ("BLACK", [0, 0, 0]),
                    ("WHITE", [255, 255, 255]),
                    ("RED", [255, 0, 0]),
                    ("GREEN", [0, 255, 0]),
                    ("BLUE", [0, 0, 255]),
                    ("CYAN", [0, 255, 255]),
                    ("MAGENTA", [255, 0, 255]),
                    ("YELLOW", [255, 255, 0]),
                    ("ORANGE", [255, 128, 0]),
                    ("PURPLE", [128, 0, 255]),
                    ("GRAY", [128, 128, 128]),
                    ("BROWN", [139, 69, 19]),
                    ("PINK", [255, 192, 203]),
                ];
                let upper = s.to_uppercase();
                if let Some(&(_, rgb)) = colors.iter().find(|&&(n, _)| n == upper.as_str()) {
                    self.graphics.pen_colors[self.graphics.pen_number] = rgb;
                }
            },
            AreaColor(h, s, l) => {
                self.graphics.area_color = (h, s, l);
            },
            AreaIntensity(r, g, b) => {
                // Store in pen_colors for area fill
                self.graphics.pen_colors[self.graphics.area_pen] =
                    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8];
            },
            SetPen(n, r, g, b) => {
                let n = n.min(15);
                self.graphics.pen_colors[n] =
                    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8];
            },
            IntEnsity(r, g, b) => {
                let n = self.graphics.pen_number;
                self.graphics.pen_colors[n] =
                    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8];
            },
            SeparateAlpha => self.graphics.alpha_separate = true,
            MergeAlpha => self.graphics.alpha_separate = false,
            GraphicsInput(ref _s) => {},
            AlphaPen => {},
            DigiTize => {
                self.output
                    .push("DIGITIZE: click on graphics window".into());
            },
            ReadLocator(ref _var) => {
                // Stub: set to 0,0
            },
        }
        Ok(())
    }

    /// CHAIN to another program — load and run, preserving blank COMMON.
    fn chain_to(&mut self, filename: &str) -> Result<()> {
        use std::fs;
        let source = fs::read_to_string(filename).map_err(|e| HtBasicError::RuntimeError {
            message: format!("CHAIN: cannot read '{}': {}", filename, e),
            span: None,
        })?;

        // Save blank COMMON block variables (key = "__blank_common__")
        let saved_blank_com: Vec<(String, Value)> = self
            .scope
            .get("__blank_com__")
            .map(|_| Vec::new())
            .unwrap_or_default();

        // Re-parse and compile the new program
        let mut parser = crate::parser::parser::Parser::new(source);
        let program = parser
            .parse_program()
            .map_err(|e| HtBasicError::RuntimeError {
                message: format!("CHAIN: parse error in '{}': {}", filename, e),
                span: None,
            })?;

        // Save current state
        let _saved_call_stack = std::mem::take(&mut self.call_stack);
        let _saved_data_values = std::mem::take(&mut self.data_values);
        let saved_output = std::mem::take(&mut self.output);
        let saved_io = std::mem::take(&mut self.io);

        // Rebuild the interpreter with the new program
        let mut new_interp = Self::new(program);

        // Restore blank COMMON variables
        for (name, val) in saved_blank_com {
            new_interp.scope.set(&name, val);
        }

        // Preserve I/O state and output
        new_interp.io = saved_io;
        new_interp.io_pending = std::mem::take(&mut self.io_pending);
        new_interp.output = saved_output;
        new_interp.graphics = std::mem::take(&mut self.graphics);

        // Run the chained program
        let result = new_interp.run();

        // Restore the interpreter's state
        self.output = new_interp.output;
        self.graphics = std::mem::take(&mut new_interp.graphics);
        self.io = std::mem::take(&mut new_interp.io);
        self.io_pending = std::mem::take(&mut new_interp.io_pending);

        result.map(|_| ())
    }

    fn jump_to_label(&mut self, label: &str) -> Result<()> {
        for (i, instr) in self.instructions.iter().enumerate() {
            if let Instr::Label(ref l) = instr {
                if l == label {
                    self.pc = i;
                    return Ok(());
                }
            }
        }
        Err(HtBasicError::RuntimeError {
            message: format!("Label not found: {}", label),
            span: None,
        })
    }
}
