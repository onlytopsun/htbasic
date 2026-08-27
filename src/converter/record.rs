//! Line-record walking for both geometries.
//!
//! Modern (`08 00`): `[len][prev][u16 linenum][u16 X][body][C9][flag]`
//! Old (`04 00`):    `[len][prev][u8 X][u16 linenum][body][C9][flag]`
//!
//! Walking relies only on `len`/`prev`/section bounds — never on flag
//! semantics — and resyncs with warnings instead of aborting.

use super::container::u16le;
use super::dialect::{Dialect, Geometry};
use super::{ConvertWarning, DecodedLine, Token};
use std::collections::BTreeMap;

pub fn decode_lines(
    sec: &[u8],
    rec_start: usize,
    slen: usize,
    dialect: Dialect,
    name_table: &[String],
    imports: &[String],
    warnings: &mut Vec<ConvertWarning>,
    unknown: &mut BTreeMap<String, usize>,
) -> Vec<DecodedLine> {
    let mut lines = Vec::new();
    let mut pos = rec_start;
    let mut prev_expected = 0usize;
    // The caller extends the section buffer past `slen` so a final truncated
    // record is walkable; everything else stays bounded by the true section
    // end (the tail check included — a terminator followed by the next
    // section's header must not masquerade as one).
    let sec_end = slen.min(sec.len());
    while pos < sec.len() {
        if pos >= sec_end {
            break;
        }
        if let Some(t) = super::section::tail_len(&sec[..sec_end], pos) {
            if t > 0 {
                warnings.push(ConvertWarning {
                    offset: pos,
                    message: "end-of-section terminator tail".into(),
                });
            }
            break;
        }
        // Stray zeros between records (seen in old-dialect files).
        let mut skipped = 0;
        while pos < sec.len() && sec[pos] == 0x00 && skipped < 2 {
            pos += 1;
            skipped += 1;
        }
        if skipped > 0 {
            warnings.push(ConvertWarning {
                offset: pos - skipped,
                message: "stray 0x00 between line records".into(),
            });
            continue;
        }
        let declared = sec[pos] as usize;
        // A declared length running past the true section end marks the
        // final record of the section as truncated (missing C9/flag — seen on
        // SUBEND records); the extension keeps header reads in-bounds.
        let avail = sec.len() - pos;
        let section_avail = sec_end.saturating_sub(pos);
        let truncated = declared > section_avail || declared > avail;
        let len = if truncated {
            section_avail.min(avail).min(declared)
        } else {
            declared
        };
        if len < 6 && !truncated {
            warnings.push(ConvertWarning {
                offset: pos,
                message: format!(
                    "invalid record length {declared} at 0x{pos:X}; stopping section"
                ),
            });
            break;
        }
        if sec.len() - pos < 6 {
            warnings.push(ConvertWarning {
                offset: pos,
                message: format!(
                    "truncated record header incomplete at 0x{pos:X}; stopping section"
                ),
            });
            break;
        }
        let prev = sec[pos + 1] as usize;
        if prev != prev_expected {
            warnings.push(ConvertWarning {
                offset: pos + 1,
                message: format!("prev-length mismatch: expected {prev_expected}, found {prev}"),
            });
        }
        let (number, indent, body_start) = match dialect.geometry {
            Geometry::Modern => (
                u32::from(u16le(&sec[pos + 2..pos + 4])),
                u16le(&sec[pos + 4..pos + 6]),
                pos + 6,
            ),
            Geometry::Old => (
                u32::from(u16le(&sec[pos + 3..pos + 5])),
                u16::from(sec[pos + 2]),
                pos + 5,
            ),
        };
        // Untruncated records end with C9 + flag; truncated ones run to the
        // true section end without them.
        let (body, flag) = if truncated {
            let end = pos + len;
            if body_start >= end {
                // Header-only stub: nothing decodable, keep the chain intact.
                (&sec[end..end], 0xFF)
            } else {
                (&sec[body_start..end], 0xFF)
            }
        } else {
            let c9_at = pos + len - 2;
            if c9_at < body_start {
                // Degenerate record: header only, no room for a body + C9.
                // Skip it rather than slicing backwards.
                warnings.push(ConvertWarning {
                    offset: pos,
                    message: format!(
                        "record length {len} has no room for a statement body; skipping"
                    ),
                });
                prev_expected = len;
                pos += len;
                continue;
            }
            if sec[c9_at] != 0xC9 {
                warnings.push(ConvertWarning {
                    offset: c9_at,
                    message: "record body not C9-terminated".into(),
                });
            }
            (&sec[body_start..c9_at], sec[pos + len - 1])
        };
        // A record body that opens with 0x00 is the jump-table tail after
        // the last real line (0x00 is never a valid token start); stop the
        // walk instead of emitting a ghost line.
        if body.first() == Some(&0x00) {
            warnings.push(ConvertWarning {
                offset: body_start,
                message: "record body starts with 0x00 (end-of-record junk?); stopping section"
                    .into(),
            });
            break;
        }
        let statements: Vec<Vec<Token>> = split_statements(body)
            .into_iter()
            .map(|s| {
                super::tokens::decode_stmt(s, &dialect, name_table, imports, warnings, unknown)
            })
            .collect();
        lines.push(DecodedLine::Tokens {
            number,
            indent,
            flag,
            statements,
        });
        prev_expected = len;
        pos += len;
    }
    lines
}

/// Split a record body into statement bodies on 0xC9 — but skip 0xC9 bytes
/// that are payload, not terminators:
/// - inside a D3 real constant (`D3 <tag> <8 bytes>`): 0.2 encodes as
///   `9A 99 99 99 99 99 C9 3F` (area color.prg lines 50/70);
/// - inside a CF string (`CF <len> <bytes>`);
/// - a comment token (01 …) runs to the end of the record — nothing after
///   it is a separate statement.
fn split_statements(body: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            0xC9 => {
                if i > start {
                    out.push(&body[start..i]);
                }
                start = i + 1;
                i += 1;
            },
            0xD3 if i + 9 < body.len() => i += 10,
            0xCF if i + 1 < body.len() => i = (i + 2 + body[i + 1] as usize).min(body.len()),
            0x01 => break,
            _ => i += 1,
        }
    }
    if start < body.len() {
        out.push(&body[start..]);
    }
    out
}
