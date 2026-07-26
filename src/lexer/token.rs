#[allow(dead_code)]

/// All token types for HTBasic / Rocky Mountain BASIC.
///
/// HTBasic is case-insensitive for keywords but preserves case for
/// string literals and variable names (though variables are matched
/// case-insensitively at runtime).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // --- Literals ---
    IntegerLiteral(i64),
    RealLiteral(f64),
    StringLiteral(String),

    // --- Identifiers and labels ---
    /// A variable or function name (without $ suffix for string type)
    Identifier(String),
    /// A string-typed identifier: name ends with $
    StringIdentifier(String),
    /// A label definition (ends with colon in source, stored without colon)
    LabelDef(String),
    /// An I/O path reference: @name
    IoPath(String),

    // --- Keywords: declarations ---
    Dim,
    Com,
    Real,
    Integer,
    Short,
    Long,
    Complex,
    Allocate,
    Deallocate,
    Redim,
    Static,
    OptionBase,

    // --- Keywords: subprograms / functions ---
    Sub,
    SubEnd,
    DefFn,
    FnEnd,
    Call,
    SubExit,
    Return,
    LoadSub,
    DelSub,

    // --- Keywords: control flow ---
    If,
    Then,
    Else,
    EndIf,
    For,
    To,
    Step,
    Next,
    While,
    EndWhile,
    Loop_,
    EndLoop,
    ExitIf,
    Repeat,
    Until,
    Select,
    Case,
    CaseElse,
    EndSelect,
    GoTo,
    GoSub,
    On,
    OnError,

    // --- Keywords: I/O ---
    Print,
    PrintUsing,
    Image,
    Input_,
    Linput,
    Assign,
    Output_,
    Enter,
    Read,
    Data,
    Restore,
    Disp,

    // --- Keywords: matrix ---
    Mat,

    // --- Keywords: other ---
    Let,
    End,
    Stop_,
    Pause,
    Rem,
    Bang, // ! comment
    Randomize,
    Wait_,
    Beep,
    Configure,
    Change,

    // --- Operators ---
    Plus,
    Minus,
    Star,
    Slash,
    Caret, // ^ exponentiation
    Amp,   // & string concatenation
    Eq,    // =
    LtGt,  // <>
    Lt,    // <
    Gt,    // >
    LtEq,  // <=
    GtEq,  // >=
    LParen,
    RParen,
    LBracket, // [
    RBracket, // ]
    Comma,
    Semicolon,
    Colon, // : multi-statement separator
    At,    // @
    Backslash,
    Dot, // .

    // --- Compound keywords ---
    And,
    Or,
    Not,
    Exor,
    Mod_,
    Modulo,
    Div_,

    // --- End of line ---
    Newline,
    /// End of file
    Eof,
}

impl TokenKind {
    /// Return a human-readable name for this token kind.
    pub fn name(&self) -> &'static str {
        match self {
            TokenKind::IntegerLiteral(_) => "integer literal",
            TokenKind::RealLiteral(_) => "real literal",
            TokenKind::StringLiteral(_) => "string literal",
            TokenKind::Identifier(_) => "identifier",
            TokenKind::StringIdentifier(_) => "string identifier",
            TokenKind::LabelDef(_) => "label definition",
            TokenKind::IoPath(_) => "I/O path",
            // Keywords
            TokenKind::Dim => "DIM",
            TokenKind::Com => "COM",
            TokenKind::Real => "REAL",
            TokenKind::Integer => "INTEGER",
            TokenKind::Short => "SHORT",
            TokenKind::Long => "LONG",
            TokenKind::Complex => "COMPLEX",
            TokenKind::Allocate => "ALLOCATE",
            TokenKind::Deallocate => "DEALLOCATE",
            TokenKind::Redim => "REDIM",
            TokenKind::Static => "STATIC",
            TokenKind::OptionBase => "OPTION BASE",
            TokenKind::Sub => "SUB",
            TokenKind::SubEnd => "SUBEND",
            TokenKind::DefFn => "DEF FN",
            TokenKind::FnEnd => "FNEND",
            TokenKind::Call => "CALL",
            TokenKind::SubExit => "SUBEXIT",
            TokenKind::Return => "RETURN",
            TokenKind::LoadSub => "LOADSUB",
            TokenKind::DelSub => "DELSUB",
            TokenKind::If => "IF",
            TokenKind::Then => "THEN",
            TokenKind::Else => "ELSE",
            TokenKind::EndIf => "END IF",
            TokenKind::For => "FOR",
            TokenKind::To => "TO",
            TokenKind::Step => "STEP",
            TokenKind::Next => "NEXT",
            TokenKind::While => "WHILE",
            TokenKind::EndWhile => "END WHILE",
            TokenKind::Loop_ => "LOOP",
            TokenKind::EndLoop => "END LOOP",
            TokenKind::ExitIf => "EXIT IF",
            TokenKind::Repeat => "REPEAT",
            TokenKind::Until => "UNTIL",
            TokenKind::Select => "SELECT",
            TokenKind::Case => "CASE",
            TokenKind::CaseElse => "CASE ELSE",
            TokenKind::EndSelect => "END SELECT",
            TokenKind::GoTo => "GOTO",
            TokenKind::GoSub => "GOSUB",
            TokenKind::On => "ON",
            TokenKind::OnError => "ON ERROR",
            TokenKind::Print => "PRINT",
            TokenKind::PrintUsing => "PRINT USING",
            TokenKind::Image => "IMAGE",
            TokenKind::Input_ => "INPUT",
            TokenKind::Linput => "LINPUT",
            TokenKind::Assign => "ASSIGN",
            TokenKind::Output_ => "OUTPUT",
            TokenKind::Enter => "ENTER",
            TokenKind::Read => "READ",
            TokenKind::Data => "DATA",
            TokenKind::Restore => "RESTORE",
            TokenKind::Disp => "DISP",
            TokenKind::Mat => "MAT",
            TokenKind::Let => "LET",
            TokenKind::End => "END",
            TokenKind::Stop_ => "STOP",
            TokenKind::Pause => "PAUSE",
            TokenKind::Rem => "REM",
            TokenKind::Bang => "!",
            TokenKind::Randomize => "RANDOMIZE",
            TokenKind::Wait_ => "WAIT",
            TokenKind::Beep => "BEEP",
            TokenKind::Configure => "CONFIGURE",
            TokenKind::Change => "CHANGE",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Caret => "^",
            TokenKind::Amp => "&",
            TokenKind::Eq => "=",
            TokenKind::LtGt => "<>",
            TokenKind::Lt => "<",
            TokenKind::Gt => ">",
            TokenKind::LtEq => "<=",
            TokenKind::GtEq => ">=",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Comma => ",",
            TokenKind::Semicolon => ";",
            TokenKind::Colon => ":",
            TokenKind::At => "@",
            TokenKind::Backslash => "\\",
            TokenKind::Dot => ".",
            TokenKind::And => "AND",
            TokenKind::Or => "OR",
            TokenKind::Not => "NOT",
            TokenKind::Exor => "EXOR",
            TokenKind::Mod_ => "MOD",
            TokenKind::Modulo => "MODULO",
            TokenKind::Div_ => "DIV",
            TokenKind::Newline => "newline",
            TokenKind::Eof => "EOF",
        }
    }
}

/// A token with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: crate::error::Span,
}

impl Token {
    pub fn new(kind: TokenKind, start: usize, end: usize) -> Self {
        Self {
            kind,
            span: crate::error::Span::new(start, end),
        }
    }
}
