#[allow(dead_code)]

/// Bytecode compiler and stack-based VM for HTBasic.
use crate::error::{HtBasicError, Result, Span};
use crate::parser::ast::*;
use crate::runtime::builtins::Builtins;
use crate::runtime::graphics::GraphicsState;
use crate::runtime::io::IoRegistry;
use crate::runtime::value::{ArrayData, Value};
use std::collections::HashMap;
use std::rc::Rc;

// ===================== OpCodes =====================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpCode {
    /// Push constant from pool: CONST <idx>
    Const(u16),
    /// Push integer literal inline (for small ints): PUSH_INT <i16>
    PushInt(i16),
    /// Push string literal from pool: PUSH_STR <idx>
    PushStr(u16),
    /// Load variable: LOAD <name_idx>
    Load(u16),
    /// Store to variable: STORE <name_idx>
    Store(u16),
    /// Load array element: ARRAY_LOAD <name_idx> (indices on stack)
    ArrayLoad(u16),
    /// Store to array element: ARRAY_STORE <name_idx> (value, indices on stack)
    ArrayStore(u16),

    // Binary ops
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Mod_,
    Modulo,
    Div_,
    // Comparison
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    // Logical
    And,
    Or,
    Not,
    Neg,
    // Truthiness test: 0/"" → 0, non-zero/non-empty → 1
    Truthy,

    // Control flow
    /// Unconditional jump: JUMP <offset>
    Jump(i32),
    /// Jump if false: JUMP_IF_FALSE <offset>
    JumpIfFalse(i32),
    /// Push current IP+1 to call stack, then jump: CALL <name_idx>, <nargs>
    Call(u16, u8),
    /// Call builtin function: BUILTIN <name_idx>, <nargs>
    Builtin(u16, u8),
    /// Return from call (pop gosub stack)
    Return,
    /// GOSUB: push IP, then jump relative
    Gosub(i32),
    /// Pop value from stack (discard)
    Pop,

    // I/O
    /// Pop value and print
    Print,
    /// Print newline
    PrintNl,
    /// Print comma (tab)
    PrintTab,
    /// Duplicate top of stack
    Dup,

    // Loops
    /// FOR loop init: FOR <var_idx> — expects start, end, step on stack
    For(u16, i32), // var name idx, jump-to-after offset
    /// NEXT loop increment: NEXT <var_idx>, <loop_start_offset>
    Next(u16, i32),

    // Subprogram
    /// Begin SUB/FN definition: ENTER_SUB <name_idx>, <nparams>
    EnterSub(u16, u8),
    /// Exit subprogram: LEAVE_SUB
    LeaveSub,
    /// Store parameter: PARAM <idx>
    Param(u8),

    // DATA/READ
    /// Push next DATA value
    ReadData,

    /// Stop execution
    Halt,
}

/// A chunk of bytecode with its constant pool.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub code: Vec<OpCode>,
    /// Constant pool: numbers, strings, variable names
    pub constants: Vec<Constant>,
    /// Line number info for error reporting (bytecode offset → source line)
    pub lines: Vec<(usize, usize)>, // (offset, line)
    /// Pre-collected DATA values for READ
    pub data_values: Vec<Value>,
}

#[derive(Debug, Clone)]
pub enum Constant {
    Real(f64),
    Integer(i64),
    String(Rc<str>),
    Name(Rc<str>),
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            lines: Vec::new(),
            data_values: Vec::new(),
        }
    }

    fn add_constant(&mut self, c: Constant) -> u16 {
        let idx = self.constants.len();
        self.constants.push(c);
        idx as u16
    }

    fn emit(&mut self, op: OpCode) {
        self.code.push(op);
    }

    fn emit_const(&mut self, c: Constant) -> u16 {
        let idx = self.add_constant(c);
        self.emit(OpCode::Const(idx));
        idx
    }
}

// ===================== Compiler =====================

pub struct Compiler {
    chunk: Chunk,
    /// Map of variable/function names → constant pool index (for fast lookup)
    name_to_idx: HashMap<String, u16>,
    /// Label → bytecode offset (resolved after compilation)
    labels: HashMap<String, usize>,
    /// Pending jumps to labels: (label_name, jump_op_offset, is_gosub)
    pending_jumps: Vec<(String, usize, bool)>,
    /// For-loop tracking: (loop_start_offset, var_idx)
    for_stack: Vec<(usize, u16)>,
    /// DATA pointer tracking
    data_values: Vec<Value>,
    /// Current data read position (set during compile)
    data_offset: usize,
    /// Builtins reference
    builtins: Builtins,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            chunk: Chunk::new(),
            name_to_idx: HashMap::new(),
            labels: HashMap::new(),
            pending_jumps: Vec::new(),
            for_stack: Vec::new(),
            data_values: Vec::new(),
            data_offset: 0,
            builtins: Builtins::new(),
        }
    }

    /// Get or create a constant pool index for a name.
    fn name_idx(&mut self, name: &str) -> u16 {
        let upper = name.to_uppercase();
        if let Some(&idx) = self.name_to_idx.get(&upper) {
            idx
        } else {
            let idx = self.chunk.add_constant(Constant::Name(Rc::from(name)));
            self.name_to_idx.insert(upper, idx);
            idx
        }
    }

    /// Compile a Program into a Chunk of bytecode.
    pub fn compile(mut self, program: Program) -> Chunk {
        // First pass: collect DATA values and register labels
        self.collect_data_and_labels(&program.statements);

        // Compile main program
        self.compile_stmts(&program.statements);

        // Compile subprograms
        for sub in &program.subprograms {
            let label_idx = self.name_idx(&sub.name);
            self.labels.insert(sub.name.clone(), self.chunk.code.len());
            self.emit(OpCode::EnterSub(label_idx, sub.params.len() as u8));
            for (i, _) in sub.params.iter().enumerate() {
                self.emit(OpCode::Param(i as u8));
            }
            self.compile_stmts(&sub.body);
            self.emit(OpCode::LeaveSub);
        }

        // Compile functions
        for func in &program.functions {
            let label_idx = self.name_idx(&func.name);
            self.labels.insert(func.name.clone(), self.chunk.code.len());
            self.emit(OpCode::EnterSub(label_idx, func.params.len() as u8));
            for (i, _) in func.params.iter().enumerate() {
                self.emit(OpCode::Param(i as u8));
            }
            self.compile_stmts(&func.body);
            self.emit(OpCode::LeaveSub);
        }

        self.emit(OpCode::Halt);

        // Resolve pending jumps
        self.resolve_jumps();

        // Pass DATA values to the chunk
        self.chunk.data_values = std::mem::take(&mut self.data_values);

        self.chunk
    }

    fn emit(&mut self, op: OpCode) {
        self.chunk.emit(op);
    }

    fn collect_data_and_labels(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            if let Stmt::Data(vals, _) = stmt {
                for v in vals {
                    match v {
                        Expr::Integer(n, _) => self.data_values.push(Value::Integer(*n)),
                        Expr::Real(n, _) => self.data_values.push(Value::Real(*n)),
                        Expr::String_(s, _) => self.data_values.push(Value::string(s)),
                        _ => {},
                    }
                }
            }
        }
    }

    fn compile_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            // Check for labels
            if let Stmt::Rem(ref msg, _) = stmt {
                if msg.starts_with("__label__") {
                    let label = msg.trim_start_matches("__label__").to_string();
                    self.labels.insert(label, self.chunk.code.len());
                    continue;
                }
            }
            self.compile_stmt(stmt);
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            // Assignment
            Stmt::Let(name, expr, _) => {
                self.compile_expr(expr);
                let ni = self.name_idx(name);
                self.emit(OpCode::Store(ni));
            },
            Stmt::ArrayAssign(name, indices, value, _) => {
                self.compile_expr(value);
                for idx in indices {
                    self.compile_expr(idx);
                }
                let ni = self.name_idx(name);
                self.emit(OpCode::ArrayStore(ni));
            },

            // DIM
            Stmt::Dim(entries, _) => {
                for entry in entries {
                    let _total: usize = entry
                        .dimensions
                        .iter()
                        .map(|&(lo, hi)| (hi - lo + 1).max(0) as usize)
                        .product();
                    let dims: Vec<(i64, i64)> = entry
                        .dimensions
                        .iter()
                        .map(|&(lo, hi)| {
                            let lower = if lo == 0 { 0 } else { lo };
                            (lower, hi)
                        })
                        .collect();
                    let _arr = ArrayData::new(dims);
                    let ni = self.name_idx(&entry.name);
                    let ci = self.chunk.add_constant(Constant::Real(0.0)); // placeholder
                    self.emit(OpCode::Const(ci));
                    self.emit(OpCode::Dup);
                    self.emit(OpCode::Store(ni));
                }
            },

            // Print
            Stmt::Print(items, _) => {
                for item in items {
                    match item {
                        PrintItem::Expr(expr) => {
                            self.compile_expr(expr);
                            self.emit(OpCode::Print);
                        },
                        PrintItem::Semicolon => {},
                        PrintItem::Comma => self.emit(OpCode::PrintTab),
                        _ => {},
                    }
                }
                self.emit(OpCode::PrintNl);
            },
            Stmt::PrintUsing(_, exprs, _) => {
                for e in exprs {
                    self.compile_expr(e);
                    self.emit(OpCode::Print);
                }
                self.emit(OpCode::PrintNl);
            },

            // Control flow
            Stmt::SingleLineIf(cond, then_stmt, else_stmt, _) => {
                self.compile_expr(cond);
                let jump_idx = self.chunk.code.len();
                self.emit(OpCode::JumpIfFalse(0));
                self.compile_stmt(then_stmt);
                if else_stmt.is_some() {
                    let skip_idx = self.chunk.code.len();
                    self.emit(OpCode::Jump(0));
                    // Patch JumpIfFalse to else part
                    let else_target = self.chunk.code.len() - jump_idx;
                    self.chunk.code[jump_idx] = OpCode::JumpIfFalse(else_target as i32);
                    self.compile_stmt(else_stmt.as_ref().unwrap());
                    let after = self.chunk.code.len() - skip_idx;
                    self.chunk.code[skip_idx] = OpCode::Jump(after as i32);
                } else {
                    let after = self.chunk.code.len() - jump_idx;
                    self.chunk.code[jump_idx] = OpCode::JumpIfFalse(after as i32);
                }
            },
            Stmt::GoTo(label, _) => {
                if let Some(&target) = self.labels.get(label) {
                    let current = self.chunk.code.len();
                    let rel = target as i32 - current as i32;
                    self.emit(OpCode::Jump(rel));
                } else {
                    let current = self.chunk.code.len();
                    self.emit(OpCode::Jump(0)); // placeholder
                    self.pending_jumps.push((label.clone(), current, false));
                }
            },
            Stmt::GoSub(label, _) => {
                if let Some(&target) = self.labels.get(label) {
                    let cur = self.chunk.code.len();
                    let rel = target as i32 - cur as i32;
                    self.emit(OpCode::Gosub(rel));
                } else {
                    let cur = self.chunk.code.len();
                    self.emit(OpCode::Gosub(0));
                    self.pending_jumps.push((label.clone(), cur, true)); // true = gosub
                }
            },
            Stmt::Return(_, _) => {
                self.emit(OpCode::Return);
            },

            Stmt::If(if_block, _) => {
                self.compile_expr(&if_block.condition);
                let jump_idx = self.chunk.code.len();
                self.emit(OpCode::JumpIfFalse(0)); // placeholder
                self.compile_stmts(&if_block.then_body);
                let end_jump_idx = self.chunk.code.len();
                self.emit(OpCode::Jump(0)); // jump past else

                // Patch jump-if-false to point past then body
                self.chunk.code[jump_idx] =
                    OpCode::JumpIfFalse((self.chunk.code.len() - jump_idx) as i32);

                let _else_offset = self.chunk.code.len();
                let mut else_patches = Vec::new();

                for (cond, body) in &if_block.else_ifs {
                    self.compile_expr(cond);
                    let ej_idx = self.chunk.code.len();
                    self.emit(OpCode::JumpIfFalse(0));
                    self.compile_stmts(body);
                    let skip_idx = self.chunk.code.len();
                    self.emit(OpCode::Jump(0));
                    self.chunk.code[ej_idx] =
                        OpCode::JumpIfFalse((self.chunk.code.len() - ej_idx) as i32);
                    else_patches.push(skip_idx);
                }

                if let Some(ref else_body) = if_block.else_body {
                    self.compile_stmts(else_body);
                }

                let after = self.chunk.code.len();
                self.chunk.code[end_jump_idx] = OpCode::Jump((after - end_jump_idx) as i32);
                for p in else_patches {
                    self.chunk.code[p] = OpCode::Jump((after - p) as i32);
                }
            },

            Stmt::For(var, start, end, step, body, _) => {
                let var_idx = self.name_idx(var);
                self.compile_expr(start);
                self.compile_expr(end);
                if let Some(ref s) = step {
                    self.compile_expr(s);
                } else {
                    let ci = self.chunk.add_constant(Constant::Real(1.0));
                    self.emit(OpCode::Const(ci));
                }
                let loop_start = self.chunk.code.len();
                self.emit(OpCode::For(var_idx, 0)); // placeholder
                let body_start = self.chunk.code.len(); // right after For
                self.compile_stmts(body);
                self.emit(OpCode::Next(var_idx, body_start as i32));
                // Patch the For instruction's exit offset (skip past Next)
                let exit = self.chunk.code.len();
                if let OpCode::For(_, ref mut off) = &mut self.chunk.code[loop_start] {
                    *off = (exit - loop_start) as i32;
                }
            },

            Stmt::While(cond, body, _) => {
                let loop_start = self.chunk.code.len();
                self.compile_expr(cond);
                let jump_idx = self.chunk.code.len();
                self.emit(OpCode::JumpIfFalse(0));
                self.compile_stmts(body);
                let current = self.chunk.code.len();
                self.emit(OpCode::Jump(loop_start as i32 - current as i32));
                let exit = self.chunk.code.len();
                self.chunk.code[jump_idx] = OpCode::JumpIfFalse((exit - jump_idx) as i32);
            },

            Stmt::Loop_(body, _) => {
                let loop_start = self.chunk.code.len();
                self.compile_stmts(body);
                self.emit(OpCode::Jump(loop_start as i32));
            },

            Stmt::Repeat(body, cond, _) => {
                let loop_start = self.chunk.code.len();
                self.compile_stmts(body);
                self.compile_expr(cond);
                self.emit(OpCode::JumpIfFalse(loop_start as i32));
            },

            Stmt::ExitIf(ref cond, _) => {
                // EXIT IF is handled in LOOP body — jump past END LOOP
                // For bytecode, compile condition and jump out
                self.compile_expr(cond);
                self.emit(OpCode::JumpIfFalse(0)); // Will be patched
            },

            // Subprogram call
            Stmt::Call(name, args, _) => {
                for arg in args {
                    self.compile_expr(arg);
                }
                let ni = self.name_idx(name);
                self.emit(OpCode::Call(ni, args.len() as u8));
            },

            // DATA/READ
            Stmt::Data(_, _) => {}, // already collected
            Stmt::Read(vars, _) => {
                for var in vars {
                    self.emit(OpCode::ReadData);
                    let ni = self.name_idx(var);
                    self.emit(OpCode::Store(ni));
                }
            },
            Stmt::Restore(_, _) => {
                // Reset data pointer — compile as special instruction
            },

            // End/Stop
            Stmt::End(_) | Stmt::Stop(_) => {
                self.emit(OpCode::Halt);
            },

            // Comments — skip
            Stmt::Comment(_, _) | Stmt::Rem(_, _) => {},

            // Graphics — compile as no-op for now (interpreted separately)
            Stmt::Gfx(_, _) => {},

            // Selection and computed branch
            Stmt::Select(_, _, _) => {},  // TODO: compile SELECT/CASE
            Stmt::OnGoTo(_, _, _) => {},  // TODO
            Stmt::OnGoSub(_, _, _) => {}, // TODO

            // Declarations
            Stmt::OptionBase(base, _) => {
                let ci = self.chunk.add_constant(Constant::Integer(*base));
                self.emit(OpCode::Const(ci));
                self.emit(OpCode::Pop); // consumed by interpreter setup
            },
            Stmt::Com(_, _) => {}, // Already handled by interpreter init

            // I/O
            Stmt::Input(_, _, _) => {},     // Stub
            Stmt::Linput(_, _, _) => {},    // Stub
            Stmt::Image(_, _) => {},        // Stub
            Stmt::Configure(_, _, _) => {}, // Stub
            Stmt::Output(_, _, _) => {},    // Stub
            Stmt::Disp(_, _) => {},         // Stub

            // String ops
            Stmt::SubStrAssign(_, _, _, _, _) => {}, // TODO

            // Matrix
            Stmt::Mat(_, _) => {}, // TODO

            // Misc
            Stmt::Beep(_) => {},
            Stmt::Wait(_, _) => {},
            Stmt::Pause(_) => {},
            Stmt::Randomize(_, _) => {},
            Stmt::Change(_, _, _, _) => {},
        }
    }

    fn compile_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Integer(n, _) => {
                if *n >= i16::MIN as i64 && *n <= i16::MAX as i64 {
                    self.emit(OpCode::PushInt(*n as i16));
                } else {
                    let ci = self.chunk.add_constant(Constant::Integer(*n));
                    self.emit(OpCode::Const(ci));
                }
            },
            Expr::Real(n, _) => {
                let ci = self.chunk.add_constant(Constant::Real(*n));
                self.emit(OpCode::Const(ci));
            },
            Expr::String_(s, _) => {
                let ci = self
                    .chunk
                    .add_constant(Constant::String(Rc::from(s.as_str())));
                self.emit(OpCode::PushStr(ci));
            },
            Expr::Variable(name, _) => {
                // Check if it's a 0-arg builtin
                if self.builtins.exists(name) {
                    let ni = self.name_idx(name);
                    self.emit(OpCode::Builtin(ni, 0));
                } else {
                    let ni = self.name_idx(name);
                    self.emit(OpCode::Load(ni));
                }
            },
            Expr::StringVariable(name, _) => {
                if self.builtins.exists(name) {
                    let ni = self.name_idx(name);
                    self.emit(OpCode::Builtin(ni, 0));
                } else {
                    let ni = self.name_idx(name);
                    self.emit(OpCode::Load(ni));
                }
            },
            Expr::WholeArray(name, _) => {
                let ni = self.name_idx(name);
                self.emit(OpCode::Load(ni));
            },
            Expr::ArrayRef(name, indices, _) => {
                for idx in indices {
                    self.compile_expr(idx);
                }
                let ni = self.name_idx(name);
                self.emit(OpCode::ArrayLoad(ni));
            },
            Expr::FnCall(name, args, _) => {
                for arg in args {
                    self.compile_expr(arg);
                }
                let ni = self.name_idx(name);
                self.emit(OpCode::Builtin(ni, args.len() as u8));
            },
            Expr::StringFnCall(name, args, _) => {
                for arg in args {
                    self.compile_expr(arg);
                }
                let ni = self.name_idx(name);
                self.emit(OpCode::Builtin(ni, args.len() as u8));
            },
            Expr::Unary(op, inner, _) => match op {
                UnaryOp::Minus => {
                    self.compile_expr(inner);
                    self.emit(OpCode::Neg);
                },
                UnaryOp::Plus => {
                    self.compile_expr(inner);
                },
                UnaryOp::Not => {
                    self.compile_expr(inner);
                    self.emit(OpCode::Not);
                },
            },
            Expr::Binary(left, op, right, _) => {
                self.compile_expr(left);
                self.compile_expr(right);
                let opcode = match op {
                    BinaryOp::Add => OpCode::Add,
                    BinaryOp::Sub => OpCode::Sub,
                    BinaryOp::Mul => OpCode::Mul,
                    BinaryOp::Div => OpCode::Div,
                    BinaryOp::Pow => OpCode::Pow,
                    BinaryOp::Concat => OpCode::Concat,
                    BinaryOp::Eq => OpCode::Eq,
                    BinaryOp::NotEq => OpCode::Neq,
                    BinaryOp::Lt => OpCode::Lt,
                    BinaryOp::Gt => OpCode::Gt,
                    BinaryOp::LtEq => OpCode::Lte,
                    BinaryOp::GtEq => OpCode::Gte,
                    BinaryOp::And => OpCode::And,
                    BinaryOp::Or => OpCode::Or,
                    BinaryOp::Exor => {
                        // XOR = (A OR B) AND NOT (A AND B) — simplify to !=
                        OpCode::Neq
                    },
                    BinaryOp::Mod_ => OpCode::Mod_,
                    BinaryOp::Modulo => OpCode::Modulo,
                    BinaryOp::Div_ => OpCode::Div_,
                };
                self.emit(opcode);
            },
            _ => {
                // SubStr etc. — push 0 for now
                let ci = self.chunk.add_constant(Constant::Real(0.0));
                self.emit(OpCode::Const(ci));
            },
        }
    }

    fn resolve_jumps(&mut self) {
        for (label, jump_offset, _is_gosub) in &self.pending_jumps {
            if let Some(&target) = self.labels.get(label) {
                let rel = target as i32 - *jump_offset as i32;
                if *jump_offset < self.chunk.code.len() {
                    match &mut self.chunk.code[*jump_offset] {
                        OpCode::Jump(ref mut offset) => *offset = rel,
                        OpCode::JumpIfFalse(ref mut offset) => *offset = rel,
                        OpCode::Gosub(ref mut offset) => *offset = rel,
                        _ => {},
                    }
                }
            }
        }
        self.pending_jumps.clear();
    }
}

// ===================== VM =====================

pub struct VM {
    pub chunk: Chunk,
    pub ip: usize,
    stack: Vec<Value>,
    /// GOSUB return address stack
    gosub_stack: Vec<usize>,
    call_stack: Vec<CallFrame>,
    /// Variable storage: variable_name → Value
    globals: HashMap<String, Value>,
    /// Built-in functions
    builtins: Builtins,
    /// DATA values for READ
    data_values: Vec<Value>,
    data_pointer: usize,
    /// Current line being built by PRINT
    current_line: String,
    /// Output buffer
    pub output: Vec<String>,
    /// Graphics state
    pub graphics: GraphicsState,
    /// I/O registry
    pub io: IoRegistry,
    /// Option base
    option_base: usize,
    /// Error state
    error_handler: Option<String>,
    pub last_err: i64,
    pub last_err_line: i64,
    pub last_err_msg: String,
}

struct CallFrame {
    return_ip: usize,
    locals: HashMap<String, Value>,
    /// For SUB/FN calls: parameter names
    params: Vec<String>,
}

impl VM {
    pub fn new(chunk: Chunk) -> Self {
        let data_values = chunk.data_values.clone();
        Self {
            chunk,
            ip: 0,
            stack: Vec::new(),
            gosub_stack: Vec::new(),
            call_stack: Vec::new(),
            globals: HashMap::new(),
            builtins: Builtins::new(),
            data_values,
            data_pointer: 0,
            current_line: String::new(),
            output: Vec::new(),
            graphics: GraphicsState::new(),
            io: IoRegistry::new(),
            option_base: 0,
            error_handler: None,
            last_err: 0,
            last_err_line: 0,
            last_err_msg: String::new(),
        }
    }

    pub fn run(&mut self) -> Result<Vec<String>> {
        loop {
            if self.ip >= self.chunk.code.len() {
                break;
            }
            let op = self.chunk.code[self.ip];
            self.ip += 1;
            self.execute(op)?;
        }

        if !self.current_line.is_empty() {
            self.output.push(std::mem::take(&mut self.current_line));
        }
        Ok(std::mem::take(&mut self.output))
    }

    fn execute(&mut self, op: OpCode) -> Result<()> {
        match op {
            OpCode::Const(idx) => {
                let val = self.constant_to_value(idx);
                self.stack.push(val);
            },
            OpCode::PushInt(n) => {
                self.stack.push(Value::Integer(n as i64));
            },
            OpCode::PushStr(idx) => {
                let val = self.constant_to_value(idx);
                self.stack.push(val);
            },
            OpCode::Load(idx) => {
                let name = self.get_name(idx).to_string();
                let val = self.lookup_var(&name).unwrap_or(Value::Real(0.0));
                self.stack.push(val);
            },
            OpCode::Store(idx) => {
                let name = self.get_name(idx).to_string();
                let val = self.stack.pop().unwrap_or(Value::Null);
                self.set_var(&name, val);
            },
            OpCode::ArrayLoad(idx) => {
                let name = self.get_name(idx).to_string();
                // Indices are on stack (in reverse order)
                let n_indices = 2; // Simplified — assumes 2D
                let mut subs = Vec::new();
                for _ in 0..n_indices {
                    subs.push(self.stack.pop().unwrap_or(Value::Null).as_integer());
                }
                subs.reverse();
                let val = self
                    .lookup_var(&name)
                    .and_then(|v| {
                        if let Value::Array(ref arr) = v {
                            arr.get(&subs).cloned()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Value::Real(0.0));
                self.stack.push(val);
            },
            OpCode::ArrayStore(idx) => {
                let name = self.get_name(idx).to_string();
                let value = self.stack.pop().unwrap_or(Value::Null);
                // Indices
                let n_indices = 2;
                let mut subs = Vec::new();
                for _ in 0..n_indices {
                    subs.push(self.stack.pop().unwrap_or(Value::Null).as_integer());
                }
                subs.reverse();
                // Store to array
                if let Some(Value::Array(ref mut arr)) = self.get_var_mut(&name) {
                    arr.set(&subs, value);
                }
            },

            // Binary ops
            OpCode::Add => self.binary_op(|a, b| Ok(Value::Real(a.as_real() + b.as_real())))?,
            OpCode::Sub => self.binary_op(|a, b| Ok(Value::Real(a.as_real() - b.as_real())))?,
            OpCode::Mul => self.binary_op(|a, b| Ok(Value::Real(a.as_real() * b.as_real())))?,
            OpCode::Div => self.binary_op(|a, b| {
                let d = b.as_real();
                if d == 0.0 {
                    Err(HtBasicError::DivisionByZero {
                        span: Span::new(0, 0),
                    })
                } else {
                    Ok(Value::Real(a.as_real() / d))
                }
            })?,
            OpCode::Pow => self.binary_op(|a, b| Ok(Value::Real(a.as_real().powf(b.as_real()))))?,
            OpCode::Concat => self.binary_op(|a, b| {
                Ok(Value::string(&format!(
                    "{}{}",
                    a.as_string(),
                    b.as_string()
                )))
            })?,
            OpCode::Mod_ => {
                self.binary_op(|a, b| Ok(Value::Integer(a.as_integer() % b.as_integer().max(1))))?
            },
            OpCode::Modulo => self.binary_op(|a, b| {
                let x = a.as_real();
                let y = b.as_real();
                if y == 0.0 {
                    Err(HtBasicError::DivisionByZero {
                        span: Span::new(0, 0),
                    })
                } else {
                    Ok(Value::Real(x - y * (x / y).floor()))
                }
            })?,
            OpCode::Div_ => {
                self.binary_op(|a, b| Ok(Value::Integer(a.as_integer() / b.as_integer().max(1))))?
            },

            // Comparison
            OpCode::Eq => self.binary_op(|a, b| {
                Ok(Value::Integer(
                    if (a.as_real() - b.as_real()).abs() < 1e-15 {
                        1
                    } else {
                        0
                    },
                ))
            })?,
            OpCode::Neq => self.binary_op(|a, b| {
                Ok(Value::Integer(
                    if (a.as_real() - b.as_real()).abs() >= 1e-15 {
                        1
                    } else {
                        0
                    },
                ))
            })?,
            OpCode::Lt => self.binary_op(|a, b| {
                Ok(Value::Integer(if a.as_real() < b.as_real() {
                    1
                } else {
                    0
                }))
            })?,
            OpCode::Gt => self.binary_op(|a, b| {
                Ok(Value::Integer(if a.as_real() > b.as_real() {
                    1
                } else {
                    0
                }))
            })?,
            OpCode::Lte => self.binary_op(|a, b| {
                Ok(Value::Integer(if a.as_real() <= b.as_real() {
                    1
                } else {
                    0
                }))
            })?,
            OpCode::Gte => self.binary_op(|a, b| {
                Ok(Value::Integer(if a.as_real() >= b.as_real() {
                    1
                } else {
                    0
                }))
            })?,

            // Logical
            OpCode::And => self.binary_op(|a, b| {
                Ok(Value::Integer(if a.is_truthy() && b.is_truthy() {
                    1
                } else {
                    0
                }))
            })?,
            OpCode::Or => self.binary_op(|a, b| {
                Ok(Value::Integer(if a.is_truthy() || b.is_truthy() {
                    1
                } else {
                    0
                }))
            })?,
            OpCode::Not => {
                let v = self.stack.pop().unwrap_or(Value::Null);
                self.stack
                    .push(Value::Integer(if v.is_truthy() { 0 } else { 1 }));
            },
            OpCode::Neg => {
                let v = self.stack.pop().unwrap_or(Value::Null);
                self.stack.push(Value::Real(-v.as_real()));
            },
            OpCode::Truthy => {
                let v = self.stack.pop().unwrap_or(Value::Null);
                self.stack
                    .push(Value::Integer(if v.is_truthy() { 1 } else { 0 }));
            },

            // Control flow
            OpCode::Jump(offset) => {
                self.ip = ((self.ip as i32) - 1 + offset) as usize;
            },
            OpCode::JumpIfFalse(offset) => {
                let v = self.stack.pop().unwrap_or(Value::Null);
                if !v.is_truthy() {
                    self.ip = ((self.ip as i32) - 1 + offset) as usize;
                }
            },
            OpCode::Builtin(name_idx, nargs) => {
                let name = self.get_name(name_idx).to_string();
                let mut args = Vec::new();
                for _ in 0..nargs {
                    args.push(self.stack.pop().unwrap_or(Value::Null));
                }
                args.reverse();
                if let Some((_, func)) = self.builtins.get_with_args(&name, nargs as usize) {
                    let result = func(&args);
                    self.stack.push(result);
                } else {
                    self.stack.push(Value::Real(0.0));
                }
            },
            OpCode::Call(name_idx, nargs) => {
                let name = self.get_name(name_idx).to_string();
                let mut args = Vec::new();
                for _ in 0..nargs {
                    args.push(self.stack.pop().unwrap_or(Value::Null));
                }
                args.reverse();
                self.call_stack.push(CallFrame {
                    return_ip: self.ip,
                    locals: HashMap::new(),
                    params: Vec::new(),
                });
                // Find and jump to subprogram
                if let Some(target) = self.find_sub(&name) {
                    // Bind parameters (stub)
                    self.ip = target;
                } else {
                    // SUB not found — might be a built-in
                    self.call_stack.pop();
                }
            },
            OpCode::Gosub(offset) => {
                self.gosub_stack.push(self.ip);
                self.ip = ((self.ip as i32) - 1 + offset) as usize;
            },
            OpCode::Return => {
                if let Some(ret_addr) = self.gosub_stack.pop() {
                    self.ip = ret_addr;
                } else if let Some(frame) = self.call_stack.pop() {
                    self.ip = frame.return_ip;
                }
            },
            OpCode::Pop => {
                self.stack.pop();
            },

            // I/O
            OpCode::Print => {
                let v = self.stack.pop().unwrap_or(Value::Null);
                self.current_line.push_str(&v.to_display_string());
            },
            OpCode::PrintNl => {
                self.output.push(std::mem::take(&mut self.current_line));
            },
            OpCode::PrintTab => {
                while self.current_line.len() % 16 != 0 {
                    self.current_line.push(' ');
                }
            },

            // FOR loop
            OpCode::For(var_idx, exit_offset) => {
                let step = self.stack.pop().unwrap_or(Value::Real(1.0));
                let end = self.stack.pop().unwrap_or(Value::Real(0.0));
                let start = self.stack.pop().unwrap_or(Value::Real(0.0));
                let s = step.as_real();
                let e = end.as_real();
                let st = start.as_real();
                let var_name = self.get_name(var_idx).to_string();
                self.set_var(&var_name, start);
                self.stack.push(end);
                self.stack.push(step);
                if (s > 0.0 && st > e) || (s < 0.0 && st < e) {
                    self.ip = ((self.ip as i32) - 1 + exit_offset) as usize;
                }
            },
            OpCode::Next(var_idx, loop_start) => {
                let step = self.stack.pop().unwrap_or(Value::Real(1.0));
                let end = self.stack.pop().unwrap_or(Value::Real(0.0));
                let var_name = self.get_name(var_idx).to_string();
                let current = self.lookup_var(&var_name).unwrap_or(Value::Real(0.0));
                let new_val = current.as_real() + step.as_real();
                self.set_var(&var_name, Value::Real(new_val));
                let s = step.as_real();
                let e = end.as_real();
                let should_continue =
                    (s > 0.0 && new_val <= e + 1e-15) || (s < 0.0 && new_val >= e - 1e-15);
                if should_continue {
                    // Push end/step back for next iteration before jumping
                    self.stack.push(end);
                    self.stack.push(step);
                    self.ip = loop_start as usize;
                }
                // Otherwise fall through (loop exits, end/step are consumed)
            },

            // Subprogram entry/exit
            OpCode::EnterSub(_, _) => {
                // Skip sub definition body in main flow
                // Find matching LeaveSub
                let mut depth = 1;
                while self.ip < self.chunk.code.len() && depth > 0 {
                    match self.chunk.code[self.ip] {
                        OpCode::EnterSub(_, _) => depth += 1,
                        OpCode::LeaveSub => depth -= 1,
                        _ => {},
                    }
                    if depth > 0 {
                        self.ip += 1;
                    }
                }
            },
            OpCode::LeaveSub => {
                // Return from subprogram
                if let Some(frame) = self.call_stack.pop() {
                    self.ip = frame.return_ip;
                }
            },
            OpCode::Param(_) => {
                // Parameter binding (stub)
            },

            // DATA/READ
            OpCode::ReadData => {
                if self.data_pointer < self.data_values.len() {
                    let val = self.data_values[self.data_pointer].clone();
                    self.data_pointer += 1;
                    self.stack.push(val);
                } else {
                    self.stack.push(Value::Real(0.0));
                }
            },

            OpCode::Halt => {
                eprintln!("  VM Halt at ip={}", self.ip - 1);
                self.ip = self.chunk.code.len();
            },

            OpCode::Dup => {
                let v = self.stack.last().cloned().unwrap_or(Value::Null);
                self.stack.push(v);
            },
        }
        Ok(())
    }

    fn binary_op<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(Value, Value) -> std::result::Result<Value, HtBasicError>,
    {
        let b = self.stack.pop().unwrap_or(Value::Null);
        let a = self.stack.pop().unwrap_or(Value::Null);
        let result = f(a, b)?;
        self.stack.push(result);
        Ok(())
    }

    fn constant_to_value(&self, idx: u16) -> Value {
        match self.chunk.constants.get(idx as usize) {
            Some(Constant::Real(n)) => Value::Real(*n),
            Some(Constant::Integer(n)) => Value::Integer(*n),
            Some(Constant::String(s)) => Value::String_(s.clone()),
            _ => Value::Null,
        }
    }

    fn get_name(&self, idx: u16) -> &str {
        match self.chunk.constants.get(idx as usize) {
            Some(Constant::Name(s)) => s.as_ref(),
            _ => "?",
        }
    }

    fn lookup_var(&self, name: &str) -> Option<Value> {
        let upper = name.to_uppercase();
        // Check call frames first (local scope)
        for frame in self.call_stack.iter().rev() {
            if let Some(v) = frame.locals.get(&upper) {
                return Some(v.clone());
            }
        }
        self.globals.get(&upper).cloned()
    }

    fn get_var_mut(&mut self, name: &str) -> Option<&mut Value> {
        let upper = name.to_uppercase();
        self.globals.get_mut(&upper)
    }

    fn set_var(&mut self, name: &str, value: Value) {
        let upper = name.to_uppercase();
        // Check local scopes first
        if let Some(frame) = self.call_stack.last_mut() {
            frame.locals.insert(upper, value);
        } else {
            self.globals.insert(upper, value);
        }
    }

    fn find_sub(&self, name: &str) -> Option<usize> {
        for (i, op) in self.chunk.code.iter().enumerate() {
            if let OpCode::EnterSub(idx, _) = op {
                if self.get_name(*idx).to_uppercase() == name.to_uppercase() {
                    return Some(i + 1); // Skip EnterSub
                }
            }
        }
        None
    }
}
