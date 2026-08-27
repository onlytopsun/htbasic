//! Render decoded containers as ASCII BASIC source.

use super::dialect::Dialect;
use super::{ConvertOptions, ConvertWarning, DecodedLine, ParsedFile, Section, Token};
use std::collections::BTreeMap;

/// Spacing class of an emitted atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sp {
    /// Words and literals: space on both sides.
    Normal,
    /// Identifier-like: no space before `(`.
    Name,
    /// `(` / `[`: no space after.
    Open,
    /// `)` / `]` / `,` / `;`: no space on either side.
    Close,
    /// Operators (`=`, `&`, `<>`, raw ASCII runs): no space on either side.
    Tight,
    /// Whole-array markers (`(*)`, `(1)`): glue to the preceding name,
    /// space after (`MAT SORT A(*) DESC`).
    Postfix,
    /// Label definition (`Finish:`): space before, none before a comment.
    LabelEnd,
    /// `!` comment: leading space, except directly after a label.
    Comment,
}

pub fn emit_source(parsed: &ParsedFile, opts: &ConvertOptions) -> String {
    let mut out = String::new();
    let mut warnings = Vec::new();
    let mut unknown = BTreeMap::new();
    for (idx, section) in parsed.sections.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        emit_section(&mut out, section, opts, &mut warnings, &mut unknown);
    }
    out
}

fn emit_section(
    out: &mut String,
    section: &Section,
    opts: &ConvertOptions,
    warnings: &mut Vec<ConvertWarning>,
    unknown: &mut BTreeMap<String, usize>,
) {
    for line in &section.lines {
        match line {
            DecodedLine::Source { number, text } => {
                out.push_str(&number.to_string());
                out.push_str(text);
                out.push('\n');
            },
            DecodedLine::Tokens {
                number,
                indent,
                statements,
                ..
            } => {
                if statements.is_empty() {
                    continue;
                }
                let rendered: Vec<String> = statements
                    .iter()
                    .filter(|st| !st.is_empty())
                    .map(|st| render_stmt(st, section, opts, warnings, unknown))
                    .collect();
                if rendered.is_empty() {
                    continue;
                }
                let width = indent_width(section, *indent);
                out.push_str(&number.to_string());
                for _ in 0..width {
                    out.push(' ');
                }
                out.push_str(&rendered.join(" : "));
                out.push('\n');
            },
        }
    }
}

/// Spaces between line number and first token.
///
/// Modern: the X field is a u16 with the space count−1 in its high byte
/// (`00 07` = 8 spaces; 0 for comment lines → 1 space); read defensively from
/// either byte in case a file stores it the other way around. Old: X is the
/// space count directly.
fn indent_width(section: &Section, indent: u16) -> usize {
    match section.geometry {
        super::dialect::Geometry::Modern => {
            let hi = indent >> 8;
            let lo = indent & 0xFF;
            (hi.max(lo) as usize) + 1
        }
        super::dialect::Geometry::Old => indent as usize,
    }
}

fn render_stmt(
    stmt: &[Token],
    section: &Section,
    opts: &ConvertOptions,
    warnings: &mut Vec<ConvertWarning>,
    unknown: &mut BTreeMap<String, usize>,
) -> String {
    let atoms = expand(stmt, section, opts, warnings, unknown);
    let mut out = String::new();
    let mut prev: Option<Sp> = None;
    for (text, sp) in atoms {
        if let Some(p) = prev {
            if needs_space(p, sp) {
                out.push(' ');
            }
        }
        out.push_str(&text);
        prev = Some(sp);
    }
    out
}

fn needs_space(prev: Sp, cur: Sp) -> bool {
    match (prev, cur) {
        // `Finish:!` — label followed directly by its comment.
        (Sp::LabelEnd, Sp::Comment) => false,
        // Inline comments always get a leading space.
        (_, Sp::Comment) => true,
        // `MAT SORT A(*) DESC`, `FOR I=A(1) TO 10` — a closing paren
        // followed by a keyword/name needs a space.
        (Sp::Close, Sp::Name) => true,
        (_, Sp::Close)
        | (Sp::Close, _)
        | (Sp::Open, _)
        | (Sp::Name, Sp::Open)
        | (_, Sp::Tight)
        | (Sp::Tight, _)
        | (_, Sp::Postfix) => false,
        _ => true,
    }
}

fn punct_spacing(p: &str) -> Sp {
    match p {
        "(" | "[" => Sp::Open,
        ")" | "]" | "," | ";" => Sp::Close,
        "(*)" | "(1)" => Sp::Postfix,
        "&" | "=" | "<>" | "+" | "-" | "*" | "/" | "^" | ":" => Sp::Tight,
        _ => Sp::Normal,
    }
}

#[allow(clippy::too_many_lines)]
fn expand(
    stmt: &[Token],
    section: &Section,
    opts: &ConvertOptions,
    warnings: &mut Vec<ConvertWarning>,
    unknown: &mut BTreeMap<String, usize>,
) -> Vec<(String, Sp)> {
    let mut atoms = Vec::new();
    let mut i = 0;
    while i < stmt.len() {
        match &stmt[i] {
            // CA operand terminator: invisible before `)`, `]`, `,`, `;`,
            // another CA, GOTO/THEN/TO/IS, or end of statement; a comma
            // elsewhere (on cycle.prg: `ON CYCLE 5 CA 42 C7 00` = ON CYCLE
            // 5 GOTO Here — no comma before the target).
            Token::Ca => {
                let suppressed = matches!(
                    stmt.get(i + 1),
                    None | Some(Token::Punct(")"))
                        | Some(Token::Punct("]"))
                        | Some(Token::Punct(","))
                        | Some(Token::Punct(";"))
                        | Some(Token::Ca)
                        | Some(Token::Kw("THEN"))
                        | Some(Token::Kw("TO"))
                        | Some(Token::Kw("IS"))
                        | Some(Token::Kw("GOTO"))
                        | Some(Token::Kw("LABEL"))
                        | Some(Token::Kw("CALL"))
                ) || matches!(
                    // Device form directly after the keyword: `GLOAD CRT,3;A(*)`
                    // (gload.prg) — no comma between GLOAD/GSTORE and the
                    // device name.
                    atoms.last(),
                    Some((t, _)) if t == "GLOAD" || t == "GSTORE"
                );
                if !suppressed {
                    atoms.push((",".to_string(), Sp::Close));
                }
            },
            Token::Dll(bytes) => {
                if let Some(text) = dll_render(&bytes[1..]) {
                    // Known DLL statement (`DLL LOAD "..."`, `DLL GET …
                    // AS …`, `DLL UNLOAD ALL`).
                    let dll = format!("DLL {text}");
                    if opts.comment_out_dll {
                        atoms.push((format!("! {dll}"), Sp::Normal));
                    } else {
                        atoms.push((dll, Sp::Normal));
                    }
                } else if opts.comment_out_dll {
                    atoms.push((format!("! DLL {}", hex_str(&bytes[1..])), Sp::Normal));
                } else {
                    // Re-decode the DLL statement body in its own dialect.
                    let dialect = Dialect::detect(section.variant, section.marker);
                    let sub = super::tokens::decode_stmt(
                        &bytes[1..],
                        &dialect,
                        &section.name_table,
                        &section.imports,
                        warnings,
                        unknown,
                    );
                    atoms.extend(expand(&sub, section, opts, warnings, unknown));
                }
            },
            Token::Kw(t) | Token::Fn(t) => {
                // LET is optional in HTBasic and omitted in saved sources
                // (`Data$=FNPaste$`), so drop it to match ground truth.
                if *t != "LET" {
                    atoms.push(((*t).to_string(), Sp::Name));
                }
            },
            Token::FnCall(n) => atoms.push((format!("FN{n}"), Sp::Name)),
            Token::Int(v) => atoms.push((v.to_string(), Sp::Normal)),
            Token::Real(r) => {
                // HTBasic writes fractional reals without the leading zero:
                // `WAIT .1` not `WAIT 0.1` (and `-.1` not `-0.1`).
                let text = format!("{r}");
                let text = match text.strip_prefix("-0.") {
                    Some(rest) => format!("-.{rest}"),
                    None => text
                        .strip_prefix("0.")
                        .map_or(text.clone(), |rest| format!(".{rest}")),
                };
                atoms.push((text, Sp::Normal));
            },
            Token::Str(s) => atoms.push((format!("\"{s}\""), Sp::Normal)),
            Token::Raw(s) => {
                // `@I`-style device paths read as names so they get spaced
                // like `OUTPUT @I;...` rather than glued to the keyword.
                let name_like = s
                    .as_bytes()
                    .first()
                    .is_some_and(|&b| b == b'@' || b.is_ascii_alphanumeric());
                atoms.push((s.clone(), if name_like { Sp::Name } else { Sp::Tight }));
            },
            Token::Var(v) | Token::LabelRef(v) => atoms.push((v.clone(), Sp::Name)),
            Token::LabelDef(n) => atoms.push((format!("{n}:"), Sp::LabelEnd)),
            Token::Punct(p) => {
                // `ASSIGN @Out TO *` — the wildcard after TO wants a space,
                // unlike the multiplication operator.
                let sp = if *p == "*" && matches!(atoms.last(), Some((t, _)) if t == "TO") {
                    Sp::Normal
                } else {
                    punct_spacing(p)
                };
                atoms.push(((*p).to_string(), sp));
            },
            Token::Comment(c) => atoms.push((format!("!{c}"), Sp::Comment)),
            Token::Unknown(bytes) => atoms.push((format!("Uh{}", hex_str(bytes)), Sp::Normal)),
            Token::UnknownStmt(bytes) => {
                atoms.push((format!("! U {}", hex_str(bytes)), Sp::Normal));
            },
        }
        i += 1;
    }
    atoms
}

fn hex_str(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render a known `DLL` statement body (`BF` prefix already stripped).
///
/// Shape (HTBClipboard.prg): opcodes with `CA` separators and `CF` string
/// literals — `5E` = LOAD, `3E` = GET (only in this context), `FF 65` =
/// UNLOAD, `FF 91` = ALL, `FF 64` = AS. Returns `None` for anything else so
/// the caller can fall back to the generic comment / re-decode path.
fn dll_render(bytes: &[u8]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // Separator between operands — rendered as whitespace only.
            0xCA => i += 1,
            // CF string literal.
            0xCF if i + 1 < bytes.len() => {
                let len = bytes[i + 1] as usize;
                let start = i + 2;
                let end = (start + len).min(bytes.len());
                let text = String::from_utf8_lossy(&bytes[start..end]).into_owned();
                parts.push(format!("\"{text}\""));
                i = end;
            },
            0x5E => {
                parts.push("LOAD".to_string());
                i += 1;
            },
            0x3E => {
                parts.push("GET".to_string());
                i += 1;
            },
            0xFF if i + 1 < bytes.len() => {
                match bytes[i + 1] {
                    0x64 => parts.push("AS".to_string()),
                    0x65 => parts.push("UNLOAD".to_string()),
                    0x91 => parts.push("ALL".to_string()),
                    _ => return None,
                }
                i += 2;
            },
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}
