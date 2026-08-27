//! Section decoding: dialect detection, adaptive first-record location,
//! name-table extraction, and record-walk dispatch.

use super::container::u16le;
use super::dialect::{Dialect, Geometry};
use super::{ConvertWarning, Section};
use std::collections::BTreeMap;

pub fn decode_section(
    sec: &[u8],
    slen: usize,
    variant: u8,
    imports: &[String],
    warnings: &mut Vec<ConvertWarning>,
    unknown: &mut BTreeMap<String, usize>,
) -> Section {
    let marker = [sec[6], sec[7]];
    let dialect = Dialect::detect(variant, marker);
    let rec_start = locate_first_record(sec, slen, dialect.geometry, sec[0]).unwrap_or_else(|| {
        let fallback = (if dialect.geometry == Geometry::Old {
            0x7D
        } else {
            0x99
        })
        .min(sec.len());
        // SUB/DEF FN sections carry a much longer preamble; look for the
        // `SUB <name>` / `DEF FN <name>` header record (`A6 C7` / `AA C7` as
        // the first two body bytes) before giving up on 0x99.
        let sig_fallback = (6..sec.len().saturating_sub(2))
            .find(|&p| {
                (sec[p] == 0xA6 || sec[p] == 0xAA) && sec[p + 1] == 0xC7 && p >= 12
            })
            .map(|p| p - 6);
        let used = if sec[0] == 1 {
            fallback
        } else {
            sig_fallback.unwrap_or(fallback)
        };
        // Main sections always fail the chain scan: the SUB-call jump table
        // trails the last line record inside slen, so 0x99/0x7D is the
        // expected preamble and not worth a warning. SUB/DEF FN sections
        // have no such tail, so a failed scan there is real signal.
        if sec[0] != 1 {
            warnings.push(ConvertWarning {
                offset: 0,
                message: format!(
                    "could not locate first line record (marker {:02X} {:02X}); assuming 0x{:X}",
                    marker[0], marker[1], used
                ),
            });
        }
        used
    });
    let name_table = parse_name_table(&sec[6..rec_start]);
    let lines = super::record::decode_lines(
        sec,
        rec_start,
        slen,
        dialect,
        &name_table,
        imports,
        warnings,
        unknown,
    );
    // Main sections register DLL-import names; later SUB sections resolve
    // `0B` call indices past their own table against this list.
    let own_imports = if sec[0] == 1 {
        extract_imports(&lines)
    } else {
        Vec::new()
    };
    Section {
        stype: sec[0],
        marker,
        geometry: dialect.geometry,
        variant,
        name_table,
        imports: own_imports,
        lines,
    }
}

/// Collect `DLL GET ... AS <name>` names from decoded DLL statements.
fn extract_imports(lines: &[super::DecodedLine]) -> Vec<String> {
    use super::Token;
    let mut names = Vec::new();
    for line in lines {
        if let super::DecodedLine::Tokens { statements, .. } = line {
            for stmt in statements {
                for tok in stmt {
                    if let Token::Dll(bytes) = tok {
                        let mut i = 0;
                        while i + 3 < bytes.len() {
                            // `FF 64` = AS, followed by a CF string literal.
                            if bytes[i] == 0xFF && bytes[i + 1] == 0x64 && bytes[i + 2] == 0xCF {
                                let len = bytes[i + 3] as usize;
                                let start = i + 4;
                                let end = (start + len).min(bytes.len());
                                names.push(
                                    String::from_utf8_lossy(&bytes[start..end]).into_owned(),
                                );
                                i = end;
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    names
}

/// Number of remaining bytes when `pos` sits at a valid end-of-section tail
/// (empty, or a `7f 00 00` / `00 00 00` terminator); `None` when the remainder
/// is not a recognized tail.
pub fn tail_len(sec: &[u8], pos: usize) -> Option<usize> {
    let rem = sec.len() - pos;
    let tail = &sec[pos..];
    match rem {
        0 => Some(0),
        3 if tail[0] == 0x7F && tail[1] == 0x00 && tail[2] == 0x00 => Some(3),
        3 if tail[0] == 0x00 && tail[1] == 0x00 && tail[2] == 0x00 => Some(3),
        _ => None,
    }
}

/// Structurally validate the record chain from `start`: every record must
/// match the previous record's length (prev-byte chain) and be C9-terminated
/// (except a truncated final record), line numbers must be non-decreasing, and
/// the chain must end at the section end `known_end` (tolerating stray zeros
/// and an optional terminator tail). Returns the number of records walked.
fn scan_records(sec: &[u8], start: usize, geometry: Geometry, known_end: usize) -> Option<usize> {
    let mut pos = start;
    let mut prev_expected = 0usize;
    let mut last_num = 0u32;
    let mut count = 0usize;
    let mut steps = 0usize;
    while pos < sec.len() {
        if steps > 1_000_000 {
            return None;
        }
        if pos >= known_end {
            return (count > 0).then_some(count);
        }
        // The caller extends the section buffer past `slen` (truncated final
        // records); terminator detection must stay bounded by the declared
        // section end, or the next section's bytes kill the chain.
        if tail_len(&sec[..known_end], pos).is_some() {
            return (count > 0).then_some(count);
        }
        // Skip at most two stray zeros between records.
        let mut skipped = 0;
        while pos < sec.len() && sec[pos] == 0x00 && skipped < 2 {
            pos += 1;
            skipped += 1;
        }
        if skipped > 0 {
            continue;
        }
        // Final records may be truncated (see record.rs); clamp to what is
        // actually present and require at least a minimal record. A truncated
        // tail record (declared length beyond the section end) may expose only
        // its header — enough to end the chain.
        let declared = sec[pos] as usize;
        let len = declared.min(sec.len() - pos);
        let truncated = len < declared;
        if len < 6 && !(truncated && len >= 5) {
            return None;
        }
        if sec[pos + 1] as usize != prev_expected {
            return None;
        }
        let num = match geometry {
            Geometry::Modern => u32::from(u16le(&sec[pos + 2..pos + 4])),
            Geometry::Old => u32::from(u16le(&sec[pos + 3..pos + 5])),
        };
        if num < last_num {
            return None;
        }
        last_num = num;
        if !truncated {
            if sec[pos + len - 2] != 0xC9 {
                return None;
            }
            prev_expected = len;
            pos += len;
            count += 1;
        } else {
            count += 1;
            return Some(count);
        }
        steps += 1;
    }
    Some(count)
}

/// Find the first line record by validating candidate starts: the best
/// candidate is the one whose structurally valid record chain (prev-links,
/// C9 terminators, non-decreasing line numbers) walks to the exact end of the
/// section with the most records. Removes any assumption about preamble length
/// (0x99 for modern files, ~0x7D for the old variant); SUB/DEF FN sections
/// carry longer preambles, so their scan range is wider.
fn locate_first_record(
    sec: &[u8],
    slen: usize,
    geometry: Geometry,
    stype: u8,
) -> Option<usize> {
    let known_end = slen.min(sec.len());
    let max_c = (if stype == 1 { 0xA8 } else { 0x400 }).min(known_end);
    let mut best: Option<(usize, usize)> = None; // (record count, offset)
    for c in 6..max_c {
        if sec[c] < 7 {
            continue;
        }
        if let Some(count) = scan_records(sec, c, geometry, known_end) {
            if best.map_or(true, |(bc, _)| count > bc) {
                best = Some((count, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// Extract the name table at the tail of the preamble: `[u8 len][ASCII]`
/// records followed by up to three zero bytes. Returns the first (complete)
/// table found; empty when none validates.
fn parse_name_table(preamble: &[u8]) -> Vec<String> {
    let end = preamble.len().saturating_sub(4);
    for start in 6..end {
        if let Some(names) = walk_names(preamble, start) {
            return names;
        }
    }
    Vec::new()
}

fn walk_names(preamble: &[u8], start: usize) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut pos = start;
    loop {
        let rem = preamble.len() - pos;
        if rem == 0 {
            return Some(names);
        }
        if rem <= 3 && preamble[pos..].iter().all(|&b| b == 0) {
            return Some(names);
        }
        let l = preamble[pos] as usize;
        if l == 0 {
            // Zero padding after the table: any length, but it must run to
            // the end of the preamble (a mid-preamble zero means a wrong start).
            return preamble[pos..]
                .iter()
                .all(|&b| b == 0)
                .then(|| names.clone());
        }
        if l > 40 || pos + 1 + l > preamble.len() {
            return None;
        }
        let name = &preamble[pos + 1..pos + 1 + l];
        if !name.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
            return None;
        }
        names.push(String::from_utf8_lossy(name).into_owned());
        pos += 1 + l;
    }
}
