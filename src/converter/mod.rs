//! Converter: decode TransEra HTBasic (HTBwin95) binary program containers —
//! tokenized `.prg` and ASCII `.bas` — into ASCII BASIC source text.
//!
//! The container format has no public specification; it was reverse-engineered
//! from real files. Decoding is deliberately tolerant: unknown opcodes become
//! placeholders or comments and never abort conversion.

mod cli;
mod container;
mod dialect;
mod emit;
mod record;
mod section;
mod tokens;

pub use cli::run;

use dialect::Geometry;
use std::collections::BTreeMap;
use std::fmt;

/// Magic for tokenized program images (`.prg` files).
pub const MAGIC_TOKENIZED: u16 = 0x8486;
/// Magic for ASCII source containers (`.bas` files saved by HTBasic).
pub const MAGIC_ASCII: u16 = 0x8488;
/// Magic seen on keyboard/install files — not programs.
pub const MAGIC_OTHER: u16 = 0x8487;

/// Size of the typed-file header; data always starts here.
pub const HEADER_LEN: usize = 0x100;

/// Emission options.
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    /// Emit DLL statements as `! DLL ...` comments (our interpreter has no DLL support).
    pub comment_out_dll: bool,
    /// Treat decode warnings as errors (CLI exits non-zero).
    pub strict: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            comment_out_dll: true,
            strict: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    Tokenized,
    Ascii,
}

/// A decoded container.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub kind: ContainerKind,
    /// Header variant byte (`0x08` / `0x04`); `None` for ASCII containers.
    pub variant: Option<u8>,
    pub sections: Vec<Section>,
    pub warnings: Vec<ConvertWarning>,
    /// Opcode → occurrence count for opcodes we could not decode.
    pub unknown_opcodes: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct Section {
    /// 1 = main, 2 = SUB/FN.
    pub stype: u8,
    /// First two preamble bytes (dialect marker).
    pub marker: [u8; 2],
    pub geometry: Geometry,
    /// File variant byte (0x08 / 0x04); used for dialect-sensitive re-decoding (DLL).
    pub variant: u8,
    /// Names referenced by C7/C8 index tokens (labels, variables, SUB names).
    pub name_table: Vec<String>,
    /// DLL-import names (`DLL GET ... AS <name>`, main section only) —
    /// fallback name space for `0B` call tokens whose index exceeds the
    /// section's own table.
    pub imports: Vec<String>,
    pub lines: Vec<DecodedLine>,
}

#[derive(Debug, Clone)]
pub enum DecodedLine {
    /// Verbatim source text (ASCII containers) — emitted as-is after the line number.
    Source { number: u32, text: String },
    /// Tokenized line.
    Tokens {
        number: u32,
        /// Raw X field (spacing before first token); interpretation depends on geometry.
        indent: u16,
        flag: u8,
        statements: Vec<Vec<Token>>,
    },
}

/// Decoded token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Keyword, canonical spelling: `"PRINT"`, `"END LOOP"`, ...
    Kw(&'static str),
    /// Built-in function token: `"TAB"`, `"CHR$"`, ...
    Fn(&'static str),
    /// User-defined FN call (`F8` token): emitted as `FN<name>`.
    FnCall(String),
    Int(i64),
    Real(f64),
    Str(String),
    /// Untokenized ASCII run (operators, unregistered names) — emitted verbatim.
    Raw(String),
    Var(String),
    LabelRef(String),
    LabelDef(String),
    Punct(&'static str),
    /// CA operand terminator — resolved with lookahead during emission.
    Ca,
    /// Comment text after `!` (may include a leading space from the original).
    Comment(String),
    /// A BF-prefixed DLL statement; raw bytes.
    Dll(Vec<u8>),
    /// Undecodable opcode bytes mid-expression → `UhXX` placeholder.
    Unknown(Vec<u8>),
    /// Undecodable whole statement → `! U <hex>` comment.
    UnknownStmt(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ConvertWarning {
    pub offset: usize,
    pub message: String,
}

#[derive(Debug)]
pub enum ConvertError {
    TooShort { offset: usize, needed: usize },
    NotAContainer { magic: u16 },
    UnsupportedContainer { magic: u16 },
    BadSectionHeader { offset: usize },
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { offset, needed } => {
                write!(
                    f,
                    "file truncated: need {needed} bytes at offset 0x{offset:X}"
                )
            },
            Self::NotAContainer { magic } => {
                write!(f, "not an HTBasic container (magic 0x{magic:04X})")
            },
            Self::UnsupportedContainer { magic } => {
                write!(
                    f,
                    "unsupported container (magic 0x{magic:04X}); keyboard/install files are not programs"
                )
            },
            Self::BadSectionHeader { offset } => {
                write!(f, "bad section header at offset 0x{offset:X}")
            },
        }
    }
}

impl std::error::Error for ConvertError {}

/// Decode a TransEra container into structured form.
pub fn decode(bytes: &[u8]) -> Result<ParsedFile, ConvertError> {
    container::decode(bytes)
}

/// Render a decoded container as ASCII BASIC source.
pub fn emit_source(parsed: &ParsedFile, opts: &ConvertOptions) -> String {
    emit::emit_source(parsed, opts)
}
