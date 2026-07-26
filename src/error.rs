#[allow(dead_code)]
use miette::Diagnostic;
use thiserror::Error;

/// A span in source code, measured in byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Return the source text covered by this span.
    pub fn source<'s>(&self, src: &'s str) -> &'s str {
        &src[self.start..self.end]
    }
}

// Implement From for miette SourceSpan conversion
impl From<Span> for miette::SourceSpan {
    fn from(span: Span) -> Self {
        miette::SourceSpan::new(span.start.into(), (span.end - span.start).into())
    }
}

#[derive(Error, Debug, Diagnostic)]
pub enum HtBasicError {
    #[error("Lexer error: {message}")]
    #[diagnostic(code(htbasic::lexer), help("Check for unexpected characters"))]
    LexError {
        message: String,
        #[label("here")]
        span: Span,
    },

    #[error("Parse error: expected {expected}, found {found}")]
    #[diagnostic(code(htbasic::parser))]
    ParseError {
        expected: String,
        found: String,
        #[label("unexpected token")]
        span: Span,
    },

    #[error("Runtime error: {message}")]
    #[diagnostic(code(htbasic::runtime))]
    RuntimeError {
        message: String,
        #[label("error occurred here")]
        span: Option<Span>,
    },

    #[error("Undefined variable: {name}")]
    #[diagnostic(
        code(htbasic::undefined_var),
        help("Declare with DIM, COM, INTEGER, or REAL")
    )]
    UndefinedVariable {
        name: String,
        #[label("not defined")]
        span: Span,
    },

    #[error("Type mismatch: {message}")]
    #[diagnostic(code(htbasic::type_error))]
    TypeError {
        message: String,
        #[label("type error")]
        span: Span,
    },

    #[error("Division by zero")]
    #[diagnostic(code(htbasic::div_zero))]
    DivisionByZero {
        #[label("division by zero")]
        span: Span,
    },

    #[error("Subscript out of range: {message}")]
    #[diagnostic(code(htbasic::subscript))]
    SubscriptError {
        message: String,
        #[label("out of range")]
        span: Span,
    },

    #[error("{message}")]
    #[diagnostic(code(htbasic::general))]
    General {
        message: String,
        #[label("")]
        span: Option<Span>,
    },
}

pub type Result<T> = std::result::Result<T, HtBasicError>;
