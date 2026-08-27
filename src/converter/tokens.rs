//! Per-dialect token decoding. Unknown opcodes degrade to placeholders or
//! whole-statement comments — decoding never hard-fails.

use super::container::u16le;
use super::dialect::{Dialect, Geometry};
use super::{ConvertWarning, Token};
use std::collections::BTreeMap;

/// What kind of source construct a spec renders as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecKind {
    Kw,
    Fn,
    Punct,
}

/// A single decoded opcode: canonical spelling + kind.
#[derive(Debug, Clone, Copy)]
pub struct TokenSpec {
    pub kind: SpecKind,
    pub text: &'static str,
}

impl TokenSpec {
    const fn kw(text: &'static str) -> Self {
        Self {
            kind: SpecKind::Kw,
            text,
        }
    }
    const fn func(text: &'static str) -> Self {
        Self {
            kind: SpecKind::Fn,
            text,
        }
    }
    const fn punct(text: &'static str) -> Self {
        Self {
            kind: SpecKind::Punct,
            text,
        }
    }
}

/// Keywords shared by all tokenized dialects (validated on ≥2 files per dialect).
pub const COMMON_SINGLE: &[(u8, TokenSpec)] = &[
    (0x03, TokenSpec::kw("ABORT")),
    (0x05, TokenSpec::kw("ALPHA")),
    (0x06, TokenSpec::kw("AREA")),
    (0x07, TokenSpec::kw("ASSIGN")),
    (0x08, TokenSpec::kw("AXES")),
    (0x09, TokenSpec::kw("BEEP")),
    (0x0C, TokenSpec::kw("CALL")),
    (0x0E, TokenSpec::kw("CAT")),
    (0x0F, TokenSpec::kw("CAUSE")),
    (0x15, TokenSpec::kw("CLEAR")),
    (0x16, TokenSpec::kw("CLIP")),
    (0x18, TokenSpec::kw("COMPLEX")),
    (0x19, TokenSpec::kw("COM")),
    (0x1A, TokenSpec::kw("CONFIGURE")),
    (0x1B, TokenSpec::kw("CONTROL")),
    (0x1F, TokenSpec::kw("CREATE")),
    (0x20, TokenSpec::kw("CSIZE")),
    (0x25, TokenSpec::kw("DEG")),
    (0x28, TokenSpec::kw("DIGITIZE")),
    (0x29, TokenSpec::kw("DIM")),
    (0x2B, TokenSpec::kw("DISPLAY")),
    (0x2C, TokenSpec::kw("DISP")),
    (0x2D, TokenSpec::kw("DRAW")),
    (0x2E, TokenSpec::kw("DUMP")),
    (0x30, TokenSpec::kw("ELSE")),
    (0x32, TokenSpec::kw("ENABLE")),
    (0x33, TokenSpec::kw("END")),
    (0x34, TokenSpec::kw("ENTER")),
    (0x35, TokenSpec::kw("ERROR")),
    (0x3B, TokenSpec::kw("FRAME")),
    (0x3C, TokenSpec::kw("GCLEAR")),
    (0x3F, TokenSpec::kw("GINIT")),
    (0x41, TokenSpec::kw("GOSUB")),
    (0x42, TokenSpec::kw("GOTO")),
    (0x43, TokenSpec::kw("GRAPHICS")),
    (0x44, TokenSpec::kw("GRID")),
    (0x48, TokenSpec::kw("IDRAW")),
    (0x49, TokenSpec::kw("IF")),
    (0x4A, TokenSpec::kw("IF")),
    (0x4B, TokenSpec::kw("IMAGE")),
    (0x4C, TokenSpec::kw("IMOVE")),
    (0x4F, TokenSpec::kw("INPUT")),
    (0x50, TokenSpec::kw("INTEGER")),
    (0x51, TokenSpec::kw("IPLOT")),
    (0x52, TokenSpec::kw("KBD")), // word form in `ON KBD` / `KBD CMODE` (kbd.prg)
    (0x53, TokenSpec::kw("KEY")),
    (0x54, TokenSpec::kw("LABEL")),
    (0x55, TokenSpec::kw("LDIR")),
    (0x56, TokenSpec::kw("LET")),
    (0x59, TokenSpec::kw("LINE")),
    (0x5C, TokenSpec::kw("LIST")),
    (0x5E, TokenSpec::kw("LOAD")),
    (0x60, TokenSpec::kw("LOCK")),
    (0x61, TokenSpec::kw("LOOP")),
    (0x62, TokenSpec::kw("LORG")),
    (0x65, TokenSpec::kw("MERGE")),
    (0x67, TokenSpec::kw("MOVE")),
    (0x68, TokenSpec::kw("MSI")),
    (0x69, TokenSpec::kw("NEXT")),
    (0x6A, TokenSpec::kw("OFF")),
    (0x6B, TokenSpec::kw("ON")),
    (0x6E, TokenSpec::kw("OUT")),
    (0x6F, TokenSpec::kw("OUTW")),
    (0x70, TokenSpec::kw("OUTPUT")),
    (0x71, TokenSpec::kw("PASS")),
    (0x72, TokenSpec::kw("PAUSE")),
    (0x73, TokenSpec::kw("PDIR")),
    (0x74, TokenSpec::kw("PENUP")),
    (0x75, TokenSpec::kw("PEN")),
    (0x77, TokenSpec::kw("PIVOT")),
    (0x78, TokenSpec::kw("PLOTTER")),
    (0x79, TokenSpec::kw("PLOT")),
    (0x7A, TokenSpec::kw("POLYGON")),
    (0x7B, TokenSpec::kw("POLYLINE")),
    (0x7E, TokenSpec::kw("PRINTER")),
    (0x7F, TokenSpec::kw("PRINT")),
    (0x80, TokenSpec::kw("PROTECT")),
    (0x81, TokenSpec::kw("PURGE")),
    (0x83, TokenSpec::kw("RAD")),
    (0x84, TokenSpec::kw("RANDOMIZE")),
    (0x85, TokenSpec::kw("READ")),
    (0x86, TokenSpec::kw("REDIM")),
    (0x87, TokenSpec::kw("RECTANGLE")),
    (0x8A, TokenSpec::kw("REM")),
    (0x8D, TokenSpec::kw("REPEAT")),
    (0x90, TokenSpec::kw("RESET")),
    (0x91, TokenSpec::kw("RESTORE")),
    (0x93, TokenSpec::kw("RESUME")),
    (0x94, TokenSpec::kw("RETURN")),
    (0x95, TokenSpec::kw("RPLOT")),
    (0x97, TokenSpec::kw("SAVE")),
    (0x9C, TokenSpec::kw("SEPARATE")),
    (0x9D, TokenSpec::kw("SET")),
    (0x9F, TokenSpec::kw("SIGNAL")),
    (0xA2, TokenSpec::kw("STOP")),
    (0xA3, TokenSpec::kw("STORE")),
    (0xA4, TokenSpec::kw("SUBEND")),
    (0xA5, TokenSpec::kw("SUBEXIT")),
    (0xA6, TokenSpec::kw("SUB")),
    (0xA7, TokenSpec::kw("SUSPEND")),
    (0xA9, TokenSpec::kw("SYMBOL")),
    (0xAB, TokenSpec::kw("SYSTEM")),
    // TRACE (trace.prg: `AD FF 91` = TRACE ALL, `D1 AD 6A` = THEN TRACE OFF
    // — TRACE EXAMPLE listing).
    (0xAD, TokenSpec::kw("TRACE")),
    (0xAE, TokenSpec::kw("TRACK")),
    (0xB1, TokenSpec::kw("UNLOCK")),
    (0xB3, TokenSpec::kw("USER")),
    (0xB4, TokenSpec::kw("VIEWPORT")),
    (0xB5, TokenSpec::kw("WAIT")),
    (0xB8, TokenSpec::kw("WINDOW")),
    (0xB9, TokenSpec::kw("WRITEIO")),
    (0xC1, TokenSpec::kw("GFONT")),
    (0xD1, TokenSpec::kw("THEN")),
    (0xD4, TokenSpec::kw("TO")),
    (0xD7, TokenSpec::kw("USING")),
    (0xD9, TokenSpec::kw("CRT")),
    (0xDC, TokenSpec::kw("KBD")),
    (0xDD, TokenSpec::kw("IS")),
    // `>` (off cycle.prg: `TIMEDATE F1 CB 05 F6 C7 01 CA D1` = TIMEDATE-5
    // >Start THEN — OFF CYCLE EXAMPLE listing; `<>` is 0xF5).
    (0xF6, TokenSpec::punct(">")),
    (0xF9, TokenSpec::kw("OR")),
];

/// Keywords specific to the 04 00 (Old) dialect.
pub const OLD_SINGLE: &[(u8, TokenSpec)] = &[
    (0x72, TokenSpec::kw("CLEAR")),
    (0x90, TokenSpec::kw("CLEAR")),
];

/// Built-in function tokens.
pub const FN_SINGLE: &[(u8, TokenSpec)] = &[
    (0xD8, TokenSpec::func("TABXY")),
    (0xDB, TokenSpec::func("TAB")),
    (0xF7, TokenSpec::func("CHR$")),
];

/// Multi-byte tokens: `FF <second>`.
pub const FF_TABLE: &[(u8, TokenSpec)] = &[
    (0x00, TokenSpec::punct("+")), // unary plus: `R(1)=+1` (output.prg)
    (0x01, TokenSpec::punct("^")),
    (0x04, TokenSpec::func("ABS")),
    (0x05, TokenSpec::func("ACS")),
    (0x06, TokenSpec::func("ACSH")),
    (0x07, TokenSpec::func("ARG")),
    (0x09, TokenSpec::func("ASNH")),
    (0x0A, TokenSpec::func("ATN2")),
    (0x0C, TokenSpec::func("ATNH")),
    (0x0D, TokenSpec::func("BASE")),
    // BINAND (binand.prg: `Z=BINAND(X,Y) !Do a binary AND of X and Y.` —
    // BINAND EXAMPLE listing line 40)
    (0x0E, TokenSpec::func("BINAND")),
    // BIT (binand.prg SUB section: `Temp=BIT(X,Loop) !Print out the answer
    // in bits.` — BIT EXAMPLE listing line 80)
    (0x14, TokenSpec::func("BIT")),
    (0x16, TokenSpec::func("CMPLX")),
    (0x18, TokenSpec::func("COS")),
    (0x19, TokenSpec::func("COSH")),
    (0x1A, TokenSpec::func("DATE")),
    (0x1B, TokenSpec::func("DATE$")),
    (0x1C, TokenSpec::func("DET")),
    (0x1D, TokenSpec::func("DIV")),
    (0x1F, TokenSpec::func("DROUND")),
    (0x20, TokenSpec::func("DVAL")),
    (0x21, TokenSpec::func("DVAL$")),
    (0x22, TokenSpec::func("CMPLX")),
    // ERRL (errl.prg: `IF FF 23 E0 <C7 ref> E1 THEN` = `IF ERRL(Here) THEN`
    // — ERRL EXAMPLE listing line 60; FF 80 is also ERRL in other files).
    (0x23, TokenSpec::func("ERRL")),
    (0x25, TokenSpec::func("EXP")),
    // FIX (fix.prg: `PRINT FF 26 E0 32 E1` = `PRINT FIX(3.2)` — FIX EXAMPLE).
    (0x26, TokenSpec::func("FIX")),
    (0x28, TokenSpec::func("IMAG")),
    (0x29, TokenSpec::func("INP")),
    (0x2A, TokenSpec::func("INPW")),
    // INT — both single-byte 0x2B (DISPLAY) and the FF 2B pair occur:
    // modulo.prg uses `FF 2B E0 X/Y E1` = INT(X/Y) (MODULO EXAMPLE listing
    // line 50), chr$.prg uses bare `2B FF B6 6B` = DISPLAY FUNCTIONS ON.
    (0x2B, TokenSpec::func("INT")),
    (0x2C, TokenSpec::func("IVAL")),
    (0x2F, TokenSpec::func("LOG")),
    // MAX / MIN (maxmin.prg: `FF 31 E0 … E1` = MAX(A(*)) — MAX/MIN EXAMPLES).
    (0x31, TokenSpec::func("MAX")),
    (0x33, TokenSpec::func("MIN")),
    (0x34, TokenSpec::func("MOD")),
    (0x35, TokenSpec::func("MODULO")),
    // NOT (not.prg: `PRINT "Not 1 is"; FF 36 CB 01` = `NOT 1` — NOT EXAMPLE
    // listing; else.prg: `IF FF 36 CB 01 THEN` = `IF NOT 1 THEN` — ELSE
    // EXAMPLE listing). `<>` is single-byte 0xF5.
    (0x36, TokenSpec::kw("NOT")),
    // Relational operators used bare in CASE clauses (case.prg:
    // `CASE FF FC 1, FF F6 100` = `CASE <1,>100` — CASE EXAMPLE listing).
    (0xFC, TokenSpec::punct("<")),
    (0xF6, TokenSpec::punct(">")),
    // Binary AND (and.prg: `PRINT J,K,J FF FB K` = `PRINT J,K,J AND K` —
    // AND EXAMPLE listing).
    (0xFB, TokenSpec::kw("AND")),
    (0x37, TokenSpec::func("NUM$")),
    (0x3C, TokenSpec::func("READIO")),
    (0x3D, TokenSpec::func("REAL")),
    (0x44, TokenSpec::func("SIN")),
    (0x45, TokenSpec::func("SINH")),
    (0x46, TokenSpec::func("SIZE")),
    (0x48, TokenSpec::func("SQR")),
    (0x49, TokenSpec::kw("STATUS")),
    (0x4B, TokenSpec::func("SYSTEM$")),
    // TIME / TIME$ (set time.prg: `FF F0 FF 4E` and `FF 4E` after SET;
    // on time.prg: `ON FF 4E …` = ON TIME …).
    (0x4E, TokenSpec::kw("TIME")),
    (0x4F, TokenSpec::func("TIME$")),
    (0x52, TokenSpec::func("VAL")),
    (0x53, TokenSpec::func("VAL$")),
    // INMEM (inmem.prg: `FF 54 E0 <C7 ref> E1` = INMEM(Method$) — INMEM EXAMPLE).
    (0x54, TokenSpec::func("INMEM")),
    (0x64, TokenSpec::kw("AS")),
    (0x65, TokenSpec::kw("UNLOAD")),
    (0x6F, TokenSpec::kw("APPEND")),
    (0x74, TokenSpec::kw("LONGFILENAMES")),
    (0x77, TokenSpec::func("CSUM")),
    (0x7B, TokenSpec::func("TRN")),
    (0x80, TokenSpec::func("ERRL")),
    (0x81, TokenSpec::func("ERRN")),
    (0x82, TokenSpec::func("FRE$")),
    (0x87, TokenSpec::kw("OPTIONAL")),
    (0x88, TokenSpec::func("PI")),
    (0x89, TokenSpec::kw("PRT")),
    (0x8C, TokenSpec::func("RND")),
    (0x8D, TokenSpec::func("TIMEDATE")),
    (0x8E, TokenSpec::func("ERRM$")),
    (0x90, TokenSpec::func("KBD$")),
    (0x91, TokenSpec::kw("ALL")),
    (0x92, TokenSpec::kw("ASCII")),
    (0x95, TokenSpec::kw("BIN")),
    (0x96, TokenSpec::kw("BUFFER")),
    (0x97, TokenSpec::kw("BYTE")),
    // BY (mat reorder.prg: `64 FF E0 C7 00 D5 FF 98 C7 01 D5 DE CB 02` =
    // `MAT REORDER Matrix BY Vector,2` — MAT REORDER EXAMPLE listing uses
    // BY; FOR loops use single-byte 0xD4 for TO instead).
    (0x98, TokenSpec::kw("BY")),
    (0x9B, TokenSpec::kw("CMODE")),
    (0x9D, TokenSpec::kw("COLOR")),
    (0x9E, TokenSpec::kw("CONDITIONAL")),
    // CYCLE (on cycle.prg: `ON FF A1 CB 05 42 C7 00` = ON CYCLE 5 GOTO Here
    // — CYCLE EXAMPLE listing).
    (0xA1, TokenSpec::kw("CYCLE")),
    (0xA3, TokenSpec::kw("DELAY")),
    (0xA6, TokenSpec::kw("DESC")),
    (0xAA, TokenSpec::kw("EDGE")),
    // EOL / TRACE (eol.prg: `ON FF AB …` = ON EOL …; trace.prg:
    // `FF AD CB 02` = TRACE 2 — TRACE EXAMPLE listing).
    (0xAB, TokenSpec::kw("EOL")),
    (0xAD, TokenSpec::kw("TRACE")),
    (0xB0, TokenSpec::kw("EXTEND")),
    (0xB1, TokenSpec::kw("FILL")),
    (0xB3, TokenSpec::kw("FORMAT")),
    (0xB5, TokenSpec::kw("FROM")),
    // FUNCTIONS (chr$.prg: `2B FF B6 6B` = DISPLAY FUNCTIONS ON).
    (0xB6, TokenSpec::kw("FUNCTIONS")),
    (0xB9, TokenSpec::kw("HEADER")),
    (0xBA, TokenSpec::kw("HEIGHT")),
    (0xBD, TokenSpec::kw("INTERACTIVE")),
    (0xBE, TokenSpec::kw("INTR")),
    (0xC1, TokenSpec::kw("KEYS")),
    // KNOB / LABELS (on knob.prg / on key.prg: `ON FF C2 … GOTO …`,
    // `KEY FF C3 OFF` = KEY LABELS OFF — AXES EXAMPLE listing).
    (0xC2, TokenSpec::kw("KNOB")),
    (0xC3, TokenSpec::kw("LABELS")),
    (0xCC, TokenSpec::kw("MAP")),
    (0xD1, TokenSpec::kw("NAMES")),
    (0xD3, TokenSpec::kw("NO")),
    (0xD7, TokenSpec::kw("OPTIONAL")),
    (0xDD, TokenSpec::kw("PRIORITY")),
    (0xDF, TokenSpec::kw("GOTO")),
    (0xE0, TokenSpec::kw("REORDER")),
    (0xE3, TokenSpec::kw("SCREEN")),
    (0xE8, TokenSpec::kw("SORT")),
    (0xEB, TokenSpec::kw("STEP")),
    (0xF1, TokenSpec::kw("TIMEDATE")),
    (0xF2, TokenSpec::kw("TIMEOUT")),
    (0xF3, TokenSpec::kw("TYPE")),
    (0xF9, TokenSpec::kw("WITH")),
];

fn spec_token(spec: &TokenSpec) -> Token {
    match spec.kind {
        SpecKind::Kw => Token::Kw(spec.text),
        SpecKind::Fn => Token::Fn(spec.text),
        SpecKind::Punct => Token::Punct(spec.text),
    }
}

fn lookup_single(b: u8, dialect: &Dialect) -> Option<&'static TokenSpec> {
    let old = dialect.geometry == Geometry::Old;
    FN_SINGLE
        .iter()
        .chain(old.then_some(OLD_SINGLE).into_iter().flatten())
        .chain(COMMON_SINGLE)
        .find(|(k, _)| *k == b)
        .map(|(_, s)| s)
}

fn count_unknown(unknown: &mut BTreeMap<String, usize>, bytes: &[u8]) {
    for b in bytes {
        *unknown.entry(format!("0x{b:02X}")).or_insert(0) += 1;
    }
}

/// Trim trailing non-printables (Old-dialect comment lengths are unreliable).
fn clamp_comment(text: &[u8]) -> &[u8] {
    let end = text
        .iter()
        .rposition(|&c| (0x20..=0x7E).contains(&c) || c == b'\t')
        .map_or(0, |p| p + 1);
    &text[..end]
}

/// Resolve a name-table index: the section's own table first, then the
/// main section's DLL-import names (used by `0B` call tokens whose index
/// exceeds the local table — e.g. HTBClipboard's `Copy(Data$)`).
fn resolve_name(name_table: &[String], imports: &[String], idx: usize) -> Option<String> {
    name_table
        .get(idx)
        .cloned()
        .or_else(|| imports.get(idx - name_table.len()).cloned())
}

/// Decode one statement body (a C9-delimited byte slice) into tokens.
#[allow(clippy::too_many_lines)]
pub fn decode_stmt(
    body: &[u8],
    dialect: &Dialect,
    name_table: &[String],
    imports: &[String],
    warnings: &mut Vec<ConvertWarning>,
    unknown: &mut BTreeMap<String, usize>,
) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    // True inside a MAT statement (`64 …`): the D2 `(*)` / D5 `(1)` bytes are
    // whole-array markers there, not subscripts, and render as nothing.
    let mut mat_mode = false;
    while i < body.len() {
        let b = body[i];
        match b {
            // Comment: 01 <prefix-len A> <text-len B> <text>.
            // The B field is unreliable (observed as text, text+1, text−1
            // across files) and a `!` comment always runs to the end of its
            // statement, so take the rest of the slice and trim non-printables.
            0x01 => {
                if i + 2 >= body.len() {
                    warnings.push(ConvertWarning {
                        offset: i,
                        message: "truncated comment token".into(),
                    });
                    break;
                }
                let text = clamp_comment(&body[i + 3..]);
                out.push(Token::Comment(String::from_utf8_lossy(text).into_owned()));
                break;
            },
            0xCB if i + 1 < body.len() => {
                // Unsigned: CHR$(132) is stored as CB 84, not a negative.
                out.push(Token::Int(i64::from(body[i + 1])));
                i += 2;
            },
            0xCC if i + 2 < body.len() => {
                out.push(Token::Int(i64::from(i16::from_le_bytes([
                    body[i + 1],
                    body[i + 2],
                ]))));
                i += 3;
            },
            // 16-bit integer literal, distinct tag from CC (inp.prg:
            // `INP(CD F8 03)` = INP(1016) = &H3F8, `INP(CD E8 03)` =
            // INP(1000) = &H3E8 — the high byte 0x03 must not decode as
            // the ABORT keyword).
            0xCD if i + 2 < body.len() => {
                out.push(Token::Int(i64::from(i16::from_le_bytes([
                    body[i + 1],
                    body[i + 2],
                ]))));
                i += 3;
            },
            0xD3 if i + 9 < body.len() => {
                // Real constant: D3 <type tag> <8-byte IEEE-754 double LE>.
                // The tag byte varies (0x41 'A', 0x43 'C', ...) and is skipped.
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&body[i + 2..i + 10]);
                out.push(Token::Real(f64::from_le_bytes(bytes)));
                i += 10;
            },
            0xCF => {
                if i + 1 >= body.len() {
                    warnings.push(ConvertWarning {
                        offset: i,
                        message: "truncated string token".into(),
                    });
                    break;
                }
                let len = body[i + 1] as usize;
                let start = i + 2;
                let end = (start + len).min(body.len());
                if start + len > body.len() {
                    warnings.push(ConvertWarning {
                        offset: i,
                        message: "string token runs past end of statement".into(),
                    });
                }
                out.push(Token::Str(
                    String::from_utf8_lossy(&body[start..end]).into_owned(),
                ));
                i = end;
            },
            0xC7 if i + 1 < body.len() => {
                let idx = body[i + 1] as usize;
                let name = match name_table.get(idx) {
                    Some(n) => n.clone(),
                    None => {
                        warnings.push(ConvertWarning {
                            offset: i,
                            message: format!(
                                "variable-name index {idx} out of range (table has {} entries)",
                                name_table.len()
                            ),
                        });
                        format!("Uv{idx:02X}")
                    },
                };
                out.push(Token::Var(name));
                i += 2;
            },
            0xC8 if i + 2 < body.len() => {
                let idx = u16le(&body[i + 1..i + 3]) as usize;
                let name = name_table
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| format!("L{idx:X}"));
                if out.is_empty() {
                    out.push(Token::LabelDef(name));
                } else {
                    out.push(Token::LabelRef(name));
                }
                i += 3;
            },
            0x00 | 0xB0 | 0xC0 | 0xD0
                if i + 6 < body.len()
                    && body[i + 1] == 0x00
                    && body[i + 2] == 0xD0 =>
            {
                // FOR header: the loop opcode varies (area color.prg uses
                // 00/B0/C0/D0 across four nested loops) and is followed by
                // `00 D0` plus the internal loop pointer; the variable and
                // bounds follow as normal tokens.
                out.push(Token::Kw("FOR"));
                i += 7;
            },
            0xD0 if i + 4 < body.len() => {
                // Internal control-flow pointer: the byte offset of the
                // matching LOOP/RETURN/THEN-target record. Not source text —
                // e.g. `END LOOP` is followed by the LOOP record's offset.
                i += 5;
            },
            0x69
                if i + 7 < body.len()
                    && matches!(body[i + 1], 0xB0 | 0xC0 | 0xD0 | 0x00)
                    && body[i + 2] == 0x00
                    && body[i + 3] == 0xD0 =>
            {
                // NEXT echoes its FOR's opcode: `69 <op> 00 D0 <ptr>`, then
                // the loop variable as a normal C7/C8 reference.
                out.push(Token::Kw("NEXT"));
                i += 8;
            },
            0x3A if out.is_empty() => {
                // Leading `:` separator — emission joins statements with
                // " : " itself, so drop it.
                i += 1;
            },
            0x3A => {
                out.push(Token::Punct(":"));
                i += 1;
            },
            0xBF => {
                out.push(Token::Dll(body[i..].to_vec()));
                break;
            },
            // Call marker — the callee name (C7/C8 ref) follows and renders
            // the call. CALL targets are 1-based into the name table
            // (trn.prg: idx 3 with table [Matrix, M, Prtmat] = Prtmat;
            // HTBClipboard: idx 3 with [Copy_data, Data$, Copy] = Copy),
            // unlike C7 var refs which are 0-based. Indices past the local
            // table fall through to the main section's DLL-import names.
            0x0B => match body.get(i + 1) {
                Some(0xC7) if i + 2 < body.len() => {
                    let idx = body[i + 2] as usize;
                    let call_name = if idx >= 1 && idx - 1 < name_table.len() {
                        Some(name_table[idx - 1].clone())
                    } else {
                        imports
                            .get(idx.saturating_sub(1).saturating_sub(name_table.len()))
                            .cloned()
                    };
                    match call_name {
                        Some(name) => out.push(Token::Var(name)),
                        None => {
                            warnings.push(ConvertWarning {
                                offset: i,
                                message: format!(
                                    "call name index {idx} out of range ({} local, {} imports)",
                                    name_table.len(),
                                    imports.len()
                                ),
                            });
                            out.push(Token::Var(format!("Uv{idx:02X}")));
                        },
                    }
                    i += 3;
                },
                Some(0xC8) if i + 3 < body.len() => {
                    let idx = u16le(&body[i + 2..i + 4]) as usize;
                    match resolve_name(name_table, imports, idx) {
                        Some(name) => out.push(Token::Var(name)),
                        None => {
                            warnings.push(ConvertWarning {
                                offset: i,
                                message: format!("call label index {idx} out of range"),
                            });
                            out.push(Token::Var(format!("L{idx:X}")));
                        },
                    }
                    i += 4;
                },
                _ => i += 1,
            },
            // Multi-line DEF FN header: `24 F8 <C7/C8 name ref>` (def fn.prg,
            // fn.prg, fnend.prg, optional.prg section 3 header lines —
            // `24 F8 C7 01 E0 …` = `DEF FNAdd(A,B)`). The name follows the
            // DEF FN keyword without the call-site FN prefix, so it decodes
            // as a plain Var here, not a FnCall.
            0x24 if out.is_empty() && body.get(i + 1) == Some(&0xF8) => {
                out.push(Token::Kw("DEF FN"));
                match body.get(i + 2) {
                    Some(0xC7) if i + 3 < body.len() => {
                        let idx = body[i + 3] as usize;
                        let name = resolve_name(name_table, imports, idx).unwrap_or_else(|| {
                            warnings.push(ConvertWarning {
                                offset: i,
                                message: format!("DEF FN name index {idx} out of range"),
                            });
                            format!("Uv{idx:02X}")
                        });
                        out.push(Token::Var(name));
                        i += 4;
                    },
                    Some(0xC8) if i + 4 < body.len() => {
                        let idx = u16le(&body[i + 3..i + 5]) as usize;
                        let name = resolve_name(name_table, imports, idx).unwrap_or_else(|| {
                            warnings.push(ConvertWarning {
                                offset: i,
                                message: format!("DEF FN label index {idx} out of range"),
                            });
                            format!("L{idx:X}")
                        });
                        out.push(Token::Var(name));
                        i += 5;
                    },
                    _ => i += 1,
                }
            },
            // FNEND terminator — a line whose statement body is the single
            // byte 0x39 (def fn.prg line 90: `39`; the help listing shows
            // `FNEND`). Guarded to the full statement so a printable-ASCII
            // run containing '9' still decodes as Raw.
            0x39 if out.is_empty() && i + 1 == body.len() => {
                out.push(Token::Kw("FNEND"));
                i += 1;
            },
            // FN-call token — the callee name (C7/C8 ref) follows; rendered
            // as `FN<name>` (def fn.prg, HTBClipboard's `FNPaste$`).
            0xF8 => match body.get(i + 1) {
                Some(0xC7) if i + 2 < body.len() => {
                    let idx = body[i + 2] as usize;
                    let name = resolve_name(name_table, imports, idx).unwrap_or_else(|| {
                        warnings.push(ConvertWarning {
                            offset: i,
                            message: format!("FN name index {idx} out of range"),
                        });
                        format!("Uv{idx:02X}")
                    });
                    out.push(Token::FnCall(name));
                    i += 3;
                },
                Some(0xC8) if i + 3 < body.len() => {
                    let idx = u16le(&body[i + 2..i + 4]) as usize;
                    let name = resolve_name(name_table, imports, idx).unwrap_or_else(|| {
                        warnings.push(ConvertWarning {
                            offset: i,
                            message: format!("FN label index {idx} out of range"),
                        });
                        format!("L{idx:X}")
                    });
                    out.push(Token::FnCall(name));
                    i += 4;
                },
                _ => {
                    count_unknown(unknown, &body[i..i + 1]);
                    warnings.push(ConvertWarning {
                        offset: i,
                        message: "unknown opcode 0xF8".into(),
                    });
                    out.push(Token::Unknown(vec![0xF8]));
                    i += 1;
                },
            },
            // 0x64 stores into an array slot: `MAT <fn> <arr>` (SORT/CSUM/…),
            // `MAT <arr> = <fn>(…)`, `MAT <arr> = <arr>`, or an element store
            // like `A$(1)=("A")` (output.prg line 40). MAT forms are spotted by
            // the whole-array marker right after the LHS var, an FF keyword
            // directly after the opcode, or a `= <ff-matfn>` further on.
            0x64 => {
                let lhs_marker = match body.get(i + 1) {
                    Some(&0xC7) => body.get(i + 3),
                    Some(&0xC8) => body.get(i + 4),
                    _ => None,
                };
                let is_mat = body.get(i + 1) == Some(&0xFF)
                    || matches!(lhs_marker, Some(&0xD5) | Some(&0xD2))
                    || body[i + 1..]
                        .windows(3)
                        .any(|w| w[0] == 0xDF && w[1] == 0xFF);
                if is_mat {
                    out.push(Token::Kw("MAT"));
                    mat_mode = true;
                } else {
                    out.push(Token::Kw("LET"));
                }
                i += 1;
            },
            // DATA: `22 <len> <len bytes of raw ASCII>` — the value list
            // is stored verbatim (and.prg: `22 0F ",0,0,0,1,1,0,1,1"` for
            // `DATA 0,0,0,1,1,0,1,1`; data.prg: `22 1B "1, 2, ..., \"Hello
            // user\""`; image.prg: `22 2E "-4, 36, ..."`). A leading comma
            // separates the items from the DATA keyword and is dropped.
            0x22 if i + 1 < body.len() => {
                out.push(Token::Kw("DATA"));
                let n = body[i + 1] as usize;
                if i + 2 + n <= body.len() {
                    let mut text = String::from_utf8_lossy(&body[i + 2..i + 2 + n]).into_owned();
                    if let Some(stripped) = text.strip_prefix(',') {
                        text = stripped.to_string();
                    }
                    out.push(Token::Raw(text));
                    i += 2 + n;
                } else {
                    i += 2;
                }
            },
            // CASE / SELECT / UNTIL / WHILE: `op D0 <u32 target ptr>`; the
            // pointer would otherwise render as a variable reference. CASE
            // may be followed directly by 0x30 = ELSE (`CASE ELSE`).
            0x0D if i + 6 < body.len() && body[i + 1] == 0xD0 => {
                out.push(Token::Kw("CASE"));
                i += 6;
                if body.get(i) == Some(&0x30) {
                    out.push(Token::Kw("ELSE"));
                    i += 1;
                }
            },
            0x9A if i + 6 < body.len() && body[i + 1] == 0xD0 => {
                out.push(Token::Kw("SELECT"));
                i += 6;
            },
            // Bare SELECT without a D0 pointer: `END SELECT` (case.prg).
            0x9A => {
                out.push(Token::Kw("SELECT"));
                i += 1;
            },
            0xB2 if i + 6 < body.len() && body[i + 1] == 0xD0 => {
                out.push(Token::Kw("UNTIL"));
                i += 6;
            },
            0xB7 if i + 6 < body.len() && body[i + 1] == 0xD0 => {
                out.push(Token::Kw("WHILE"));
                i += 6;
            },
            // `06 70` / `08 70` = OUTPUT prefix inside SUB sections — the lead
            // byte carries no source text of its own.
            0x06 | 0x08 if out.is_empty() && body.get(i + 1) == Some(&0x70) => {
                i += 1;
            },
            // Statement end — the record split already removed these; tolerate.
            0xC9 => i += 1,
            0xCA => {
                out.push(Token::Ca);
                i += 1;
            },
            0xDE => {
                out.push(Token::Punct(","));
                i += 1;
            },
            0xEF => {
                out.push(Token::Punct("/"));
                i += 1;
            },
            0xE0 | 0xE8 => {
                out.push(Token::Punct("("));
                i += 1;
            },
            0xE1 | 0xE2 | 0xE7 => {
                out.push(Token::Punct(")"));
                i += 1;
            },
            0xDA => {
                out.push(Token::Punct(":"));
                i += 1;
            },
            0xE3 => {
                out.push(Token::Punct("]"));
                i += 1;
            },
            // `]` — E3 in most dialects, but E4 everywhere it appears as
            // `]` (where.prg 00 00, track.prg 02 00: `Stat$[5,5 E4`).
            // track.prg is the only file to carry an E4, so no dialect
            // guard is needed.
            0xE4 => {
                out.push(Token::Punct("]"));
                i += 1;
            },
            // 02 00 dialect statement keywords that fall in the printable
            // ASCII range elsewhere (gload.prg: `3D CRT,3;A(*)` = GLOAD,
            // `46 B(*)` / `40 B(*)` = FOR/NEXT array iteration).
            0x3D if dialect.marker == [0x02, 0x00] => {
                out.push(Token::Kw("GLOAD"));
                i += 1;
            },
            0x04 if dialect.marker == [0x02, 0x00] => {
                out.push(Token::Kw("GSTORE"));
                i += 1;
            },
            0x46 if dialect.marker == [0x02, 0x00] => {
                out.push(Token::Kw("FOR"));
                i += 1;
            },
            0x40 if dialect.marker == [0x02, 0x00] => {
                out.push(Token::Kw("NEXT"));
                i += 1;
            },
            0xE6 => {
                out.push(Token::Punct(";"));
                i += 1;
            },
            0xF3 => {
                out.push(Token::Punct("&"));
                i += 1;
            },
            0xEE => {
                out.push(Token::Punct("+"));
                i += 1;
            },
            0xF1 | 0xF4 => {
                out.push(Token::Punct("-"));
                i += 1;
            },
            0xF2 => {
                out.push(Token::Punct("*"));
                i += 1;
            },
            0xF5 => {
                // `<>` (log.prg: `IF LOG(EXP(65))<>65 THEN PRINT "Test
                // failed."` — LOG(EXP(65)) round-trips to exactly 65).
                out.push(Token::Punct("<>"));
                i += 1;
            },
            0xD2 => {
                // Entire-array marker: `A(*)` / `R(*)` (base.bas, csum.bas) —
                // suppressed inside MAT statements (whole array implied).
                if !mat_mode {
                    out.push(Token::Punct("(*)"));
                }
                i += 1;
            },
            0xD5 => {
                // Array-slot marker: `B$(1)` (dim.bas, output.prg) — suppressed
                // inside MAT statements (whole array implied).
                if !mat_mode {
                    out.push(Token::Punct("(1)"));
                }
                i += 1;
            },
            0xDF => {
                out.push(Token::Punct("="));
                i += 1;
            },
            0xF0 => {
                out.push(Token::Punct("["));
                i += 1;
            },
            // 03 00 dialect renders E9 as "[", the rest as "(" (trn.prg:
            // `DIM Matrix E9 1:3,1:3 E7` = `DIM Matrix(1:3,1:3)`).
            0xE9 if dialect.marker == [0x03, 0x00] => {
                out.push(Token::Punct("["));
                i += 1;
            },
            0xE9 => {
                out.push(Token::Punct("("));
                i += 1;
            },
            // OPTION BASE is 6d ff 93 (03 00 dialect; dim.prg uses it too).
            0x6D if i + 2 < body.len() && body[i + 1] == 0xFF && body[i + 2] == 0x93 =>
            {
                out.push(Token::Kw("OPTION BASE"));
                i += 3;
            },
            // `63 FF EC DD` = MASS STORAGE IS (cd.prg, msi.prg, from.prg,
            // protect.prg, read label.prg; DD is the IS keyword). 0x63 alone
            // is not a keyword anywhere else.
            0x63 if i + 2 < body.len() && body[i + 1] == 0xFF && body[i + 2] == 0xEC => {
                out.push(Token::Kw("MASS STORAGE"));
                i += 3;
            },
            0xFF => {
                if i + 1 >= body.len() {
                    warnings.push(ConvertWarning {
                        offset: i,
                        message: "dangling FF token".into(),
                    });
                    count_unknown(unknown, &body[i..i + 1]);
                    out.push(Token::Unknown(vec![0xFF]));
                    i += 1;
                } else if body[i + 1] == 0x70 && i + 5 < body.len() {
                    // `FF 70 <u32 LE>` = integer constant too large for the
                    // CB/CC literals (timedate.prg: 86400 = `80 51 01 00`;
                    // the DROUND example uses Number=656576 = `C0 04 0A 00`).
                    let val = u32::from_le_bytes([
                        body[i + 2],
                        body[i + 3],
                        body[i + 4],
                        body[i + 5],
                    ]);
                    out.push(Token::Int(i64::from(val)));
                    i += 6;
                } else if body[i + 1] == 0xF0 {
                    // Clock family: `FF F0` = TIME (set time.prg: `SET
                    // FF F0 FF 4E` and bare `FF F0` forms both mean TIME;
                    // on time.prg uses `FF 4E` after ON). The compound
                    // `FF F0 FF 4E` consumes four bytes, bare `FF F0` two.
                    if i + 3 < body.len() && body[i + 2] == 0xFF && body[i + 3] == 0x4E {
                        out.push(Token::Kw("TIME"));
                        i += 4;
                    } else {
                        out.push(Token::Kw("TIME"));
                        i += 2;
                    }
                } else if body[i + 1] == 0x73 {
                    // Suppressed array-function marker before `(` (maxmin.prg:
                    // `FF 31 FF 73 E0` = MAX( — the FF 73 renders as nothing).
                    i += 2;
                } else if let Some(spec) = FF_TABLE
                    .iter()
                    .find(|(k, _)| *k == body[i + 1])
                    .map(|(_, s)| s)
                {
                    out.push(spec_token(spec));
                    i += 2;
                } else {
                    count_unknown(unknown, &body[i..i + 2]);
                    warnings.push(ConvertWarning {
                        offset: i,
                        message: format!("unknown multi-byte opcode FF {:02X}", body[i + 1]),
                    });
                    if out.is_empty() {
                        out.push(Token::UnknownStmt(body[i..].to_vec()));
                        break;
                    }
                    out.push(Token::Unknown(vec![0xFF, body[i + 1]]));
                    i += 2;
                }
            },
            _ => {
                if let Some(spec) = lookup_single(b, dialect) {
                    out.push(spec_token(spec));
                    i += 1;
                } else if (0x20..=0x7E).contains(&b) {
                    // Untokenized ASCII run (operators, unregistered names).
                    let start = i;
                    while i < body.len() && (0x20..=0x7E).contains(&body[i]) {
                        i += 1;
                    }
                    out.push(Token::Raw(
                        String::from_utf8_lossy(&body[start..i]).into_owned(),
                    ));
                } else {
                    count_unknown(unknown, &body[i..i + 1]);
                    warnings.push(ConvertWarning {
                        offset: i,
                        message: format!("unknown opcode 0x{b:02X}"),
                    });
                    if out.is_empty() {
                        // An unmapped statement-opening keyword: keep the whole
                        // statement as a comment so the rest stays parseable.
                        out.push(Token::UnknownStmt(body[i..].to_vec()));
                        break;
                    }
                    out.push(Token::Unknown(vec![b]));
                    i += 1;
                }
            },
        }
    }
    out
}
