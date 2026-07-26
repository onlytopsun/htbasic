use crate::error::Span;

#[allow(dead_code)]

/// A complete HTBasic program: main body followed by subprograms and functions.
#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
    pub subprograms: Vec<SubProgram>,
    pub functions: Vec<FnDef>,
}

/// A statement in HTBasic.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// LET var = expr (LET keyword is optional)
    Let(String, Expr, Span),
    /// Array element assignment: A(i,j) = expr
    ArrayAssign(String, Vec<Expr>, Expr, Span),
    /// DIM var(dims), DIM var$(dims)
    Dim(Vec<DimEntry>, Span),
    /// COM /BlockName/ type var, ...
    Com(ComBlock, Span),
    /// PRINT [USING format;] expr [,;] expr ...
    Print(Vec<PrintItem>, Span),
    /// PRINT USING format; expr [,;] expr ...
    PrintUsing(Expr, Vec<Expr>, Span),
    /// IF cond THEN ... [ELSE ...] END IF (multi-line form)
    If(IfBlock, Span),
    /// Single-line IF: IF cond THEN stmt [ELSE stmt]
    SingleLineIf(Expr, Box<Stmt>, Option<Box<Stmt>>, Span),
    /// FOR var = start TO end [STEP step] ... NEXT var
    For(String, Expr, Expr, Option<Expr>, Vec<Stmt>, Span),
    /// WHILE cond ... END WHILE
    While(Expr, Vec<Stmt>, Span),
    /// LOOP ... [EXIT IF cond] ... END LOOP
    Loop_(Vec<Stmt>, Span),
    /// EXIT IF condition (only valid inside LOOP)
    ExitIf(Box<Expr>, Span),
    /// REPEAT ... UNTIL cond
    Repeat(Vec<Stmt>, Expr, Span),
    /// SELECT expr ... CASE ... END SELECT
    Select(Expr, Vec<CaseArm>, Span),
    /// GOTO label
    GoTo(String, Span),
    /// GOSUB label
    GoSub(String, Span),
    /// ON expr GOTO lab1, lab2, ...
    OnGoTo(Expr, Vec<String>, Span),
    /// ON expr GOSUB lab1, lab2, ...
    OnGoSub(Expr, Vec<String>, Span),
    /// RETURN [expr] (expr only in DEF FN)
    Return(Option<Expr>, Span),
    /// CALL SubName(args)
    Call(String, Vec<Expr>, Span),
    /// DATA val1, val2, ...
    Data(Vec<Expr>, Span),
    /// READ var1, var2, ...
    Read(Vec<String>, Span),
    /// RESTORE [label]
    Restore(Option<String>, Span),
    /// IMAGE format_string
    Image(String, Span),
    /// STOP
    Stop(Span),
    /// END
    End(Span),
    /// PAUSE
    Pause(Span),
    /// INPUT [prompt;] var1, var2, ...
    Input(Option<String>, Vec<String>, Span),
    /// LINPUT [prompt;] var$
    Linput(Option<String>, String, Span),
    /// DISP message
    Disp(String, Span),
    /// BEEP
    Beep(Span),
    /// WAIT seconds
    Wait(Expr, Span),
    /// RANDOMIZE [seed]
    Randomize(Option<Expr>, Span),
    /// MAT A = B + C (matrix operations)
    Mat(MatOp, Span),
    /// REM comment
    Rem(String, Span),
    /// !  comment
    Comment(String, Span),
    /// OPTION BASE 0 or 1
    OptionBase(i64, Span),
    /// CONFIGURE keyword value
    Configure(String, String, Span),
    /// CHANGE string TO array  or  CHANGE array TO string
    Change(ChangeDir, String, String, Span),
    /// Graphics command
    Gfx(GfxCmd, Span),
    /// Assignment-like: var$[start,end] = expr (substring assignment)
    SubStrAssign(String, Expr, Expr, Expr, Span),
}

/// Graphics commands for HTBasic Phase 4.
#[derive(Debug, Clone)]
pub enum GfxCmd {
    Ginit,
    Gclear,
    PlotterIs(String, String), // device, options
    Move(f64, f64),
    Draw(f64, f64, bool, bool), // x, y, relative, pen_control
    Plot(f64, f64),
    Pen(usize),
    LineType(usize),
    Label(String),
    Csize(f64, Option<f64>),
    Ldir(f64),
    Lorg(usize),
    Gfont(String),
    Axes(f64, f64, f64, f64), // xtic, ytic, xorg, yorg
    Grid(f64, f64, f64, f64),
    Frame,
    Clip(f64, f64, f64, f64),
    ClipOff,
    Window(f64, f64, f64, f64),
    Viewport(f64, f64, f64, f64),
    Rectangle(f64, f64, f64, f64, bool, bool), // x1,y1,x2,y2, fill, edge
    Polygon(Vec<(f64, f64)>),
    Polyline(Vec<(f64, f64)>),
    Gload(String),
    Gstore(String),
    Penup,
    Color(String),
    AreaColor(f64, f64, f64),     // H, S, L
    AreaIntensity(f64, f64, f64), // R, G, B
    SetPen(usize, f64, f64, f64), // pen, r, g, b
    IntEnsity(f64, f64, f64),     // r, g, b
    SeparateAlpha,
    MergeAlpha,
    GraphicsInput(String), // device
    AlphaPen,
    DigiTize,
    ReadLocator(String), // var
}

/// A DIM entry: name, dimensions [(lower, upper), ...]
#[derive(Debug, Clone)]
pub struct DimEntry {
    pub name: String,
    pub dimensions: Vec<(i64, i64)>, // (lower, upper) — lower defaults to OPTION BASE
}

/// COM block definition.
#[derive(Debug, Clone)]
pub struct ComBlock {
    pub name: Option<String>, // None = blank common
    pub entries: Vec<ComEntry>,
}

#[derive(Debug, Clone)]
pub struct ComEntry {
    pub var_type: VarType,
    pub name: String,
    pub dimensions: Vec<(i64, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    Real,
    Integer,
    Short,
    Long,
    Complex,
    String_,
}

/// IF block (multi-line).
#[derive(Debug, Clone)]
pub struct IfBlock {
    pub condition: Expr,
    pub then_body: Vec<Stmt>,
    pub else_ifs: Vec<(Expr, Vec<Stmt>)>, // ELSE IF chains
    pub else_body: Option<Vec<Stmt>>,
    pub span: Span,
}

/// A SELECT CASE arm.
#[derive(Debug, Clone)]
pub struct CaseArm {
    pub cases: Vec<CaseValue>,
    pub body: Vec<Stmt>,
}

/// Value(s) matched by a CASE clause.
#[derive(Debug, Clone)]
pub enum CaseValue {
    /// CASE expr
    Single(Expr),
    /// CASE low TO high
    Range(Expr, Expr),
    /// CASE IS op expr
    Is(RelOp, Expr),
    /// CASE ELSE
    Else,
}

/// Relational operators for CASE IS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RelOp {
    Lt,
    LtEq,
    Eq,
    GtEq,
    Gt,
    NotEq,
}

/// PRINT items.
#[derive(Debug, Clone)]
pub enum PrintItem {
    Expr(Expr),             // PRINT expr
    Tab(Expr),              // TAB(expr)
    Semicolon,              // ; suppresses newline or tabs
    Comma,                  // , tabs to next zone
    Using(Expr, Vec<Expr>), // USING format; exprs
}

/// Matrix operation variants.
#[derive(Debug, Clone)]
pub enum MatOp {
    /// MAT A = B
    Assign(String, String, Span),
    /// MAT A = B + C  (or -, *, etc.)
    Binary(String, String, MatBinOp, String, Span),
    /// MAT A = (expr) + B
    ScalarBinary(String, Expr, MatBinOp, String, Span),
    /// MAT A = INV(B), TRN(B), ZER, CON, IDN
    Func(String, MatFunc, String, Span),
    /// MAT A = ZER[(dims)]  etc.
    FuncInit(String, MatFunc, Vec<(i64, i64)>, Span),
    /// MAT INPUT A
    Input(String, Span),
    /// MAT PRINT A
    Print(String, Span),
    /// MAT READ A
    Read(String, Span),
    /// MAT A = RSUM(B) or CSUM(B)
    Reduc(String, ReducFunc, String, Span),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatBinOp {
    Add,
    Sub,
    Mul,
    Div,
    DotMul, // element-by-element multiply
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MatFunc {
    Inv,
    Trn,
    Zer,
    Con,
    Idn,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReducFunc {
    Rsum,
    Csum,
}

#[derive(Debug, Clone)]
pub enum ChangeDir {
    StringToArray,
    ArrayToString,
}

/// An expression node.
#[derive(Debug, Clone)]
pub enum Expr {
    Integer(i64, Span),
    Real(f64, Span),
    String_(String, Span),
    Variable(String, Span),
    StringVariable(String, Span),
    ArrayRef(String, Vec<Expr>, Span),
    /// Function call: FNname(args)
    FnCall(String, Vec<Expr>, Span),
    /// String function: FNname$(args)
    StringFnCall(String, Vec<Expr>, Span),
    /// Substring ref: A$[start, end] or A$[start; length]
    SubStr(String, Box<Expr>, Option<Box<Expr>>, bool, Span),
    /// Unary operator
    Unary(UnaryOp, Box<Expr>, Span),
    /// Binary operator
    Binary(Box<Expr>, BinaryOp, Box<Expr>, Span),
}

impl Expr {
    /// Helper to get an integer from a literal expression, or 0.
    pub fn as_integer_or_zero(&self) -> i64 {
        match self {
            Expr::Integer(n, _) => *n,
            Expr::Real(n, _) => *n as i64,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat, // &
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
    Exor,
    Mod_,
    Modulo,
    Div_,
}

/// A subprogram definition.
#[derive(Debug, Clone)]
pub struct SubProgram {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// A DEF FN definition.
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub returns_string: bool,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// A parameter in a SUB or DEF FN.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub param_type: ParamType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    /// Regular variable (by reference)
    Variable,
    /// Array passed with (*)
    Array,
    /// I/O path passed with @
    IoPath,
    /// String variable (has $ suffix)
    String_,
}
