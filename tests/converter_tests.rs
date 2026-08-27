//! Converter tests — synthetic containers built in code from the
//! reverse-engineered format description.
//!
//! IMPORTANT: no bytes from the copyrighted TransEra distribution appear
//! here or anywhere in the repository. Real-file comparisons live in the
//! feature-gated, `#[ignore]`d `ground_truth` module at the bottom and read
//! files outside the repo at test time (dev machine only, never CI).

use htbasic::converter::{
    decode, emit_source, ContainerKind, ConvertError, ConvertOptions, DecodedLine, ParsedFile,
    Token, HEADER_LEN, MAGIC_ASCII, MAGIC_OTHER, MAGIC_TOKENIZED,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

// ===================== synthetic encoder =====================
// Minimal writer for the container shapes described in src/converter/*.rs.

fn header(magic: u16, variant: u8, declared: u32) -> Vec<u8> {
    let mut h = vec![0u8; HEADER_LEN];
    h[0..2].copy_from_slice(&magic.to_le_bytes());
    h[8..12].copy_from_slice(&declared.to_le_bytes());
    h[0x10] = variant;
    h
}

/// Modern record: `[len][prev][u16 num][u16 X][body][C9][flag]`.
/// `spaces` = spaces between the line number and first token (0 renders as 1).
fn rec_modern(num: u16, spaces: u8, stmts: &[&[u8]], flag: u8) -> Vec<u8> {
    let mut body = Vec::new();
    for (i, s) in stmts.iter().enumerate() {
        if i > 0 {
            body.push(0xC9);
        }
        body.extend_from_slice(s);
    }
    let len = 6 + body.len() + 2;
    let mut r = Vec::with_capacity(len);
    r.push(len as u8);
    r.push(0); // prev — patched by chain_from
    r.extend_from_slice(&num.to_le_bytes());
    r.extend_from_slice(&((u16::from(spaces.saturating_sub(1))) << 8).to_le_bytes());
    r.extend_from_slice(&body);
    r.push(0xC9);
    r.push(flag);
    r
}

/// Old record: `[len][prev][u8 X][u16 num][body][C9][flag]`.
fn rec_old(num: u16, spaces: u8, stmts: &[&[u8]], flag: u8) -> Vec<u8> {
    let mut body = Vec::new();
    for (i, s) in stmts.iter().enumerate() {
        if i > 0 {
            body.push(0xC9);
        }
        body.extend_from_slice(s);
    }
    let len = 5 + body.len() + 2;
    let mut r = Vec::with_capacity(len);
    r.push(len as u8);
    r.push(0); // prev — patched by chain_from
    r.push(spaces);
    r.extend_from_slice(&num.to_le_bytes());
    r.extend_from_slice(&body);
    r.push(0xC9);
    r.push(flag);
    r
}

/// Chain records: each record's prev byte = the previous record's length.
fn chain_from(recs: &[Vec<u8>], first_prev: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut prev = first_prev;
    for r in recs {
        let mut r = r.clone();
        r[1] = prev;
        prev = r[0];
        out.extend_from_slice(&r);
    }
    out
}

fn chain(recs: &[Vec<u8>]) -> Vec<u8> {
    chain_from(recs, 0)
}

/// One tokenized section: `[type][0x10][u32 slen][preamble][records][tail]`.
///
/// Preamble = zero filler + name table (`[len][ASCII]` records) ending
/// exactly at the first record. Zero filler keeps every preamble byte below
/// the 7-byte minimum candidate length in the adaptive record-start scan,
/// and with no terminator zeros the last name byte is followed by the first
/// record's length (nonzero), so no preamble position can fake a record
/// chain — the true record start is always the lowest-offset candidate.
fn section(
    stype: u8,
    preamble_len: usize,
    names: &[&str],
    recs: &[Vec<u8>],
    tail: &[u8],
) -> Vec<u8> {
    assert!(preamble_len >= 6);
    let mut nt = Vec::new();
    for n in names {
        assert!(n.len() <= 40 && n.bytes().all(|b| (0x20..=0x7E).contains(&b)));
        nt.push(n.len() as u8);
        nt.extend_from_slice(n.as_bytes());
    }
    let mut pre = vec![0x00u8; preamble_len - 6];
    assert!(nt.len() <= pre.len(), "name table does not fit the preamble");
    let start = pre.len() - nt.len();
    pre[start..].copy_from_slice(&nt);
    let mut body = pre;
    body.extend_from_slice(&chain(recs));
    body.extend_from_slice(tail);
    let mut sec = vec![stype, 0x10];
    sec.extend_from_slice(&((6 + body.len()) as u32).to_le_bytes());
    sec.extend_from_slice(&body);
    sec
}

/// Tokenized container; declared data length 0 → EOF fallback.
fn tokenized(variant: u8, sections: &[Vec<u8>]) -> Vec<u8> {
    let mut f = header(MAGIC_TOKENIZED, variant, 0);
    f.extend_from_slice(&sections.concat());
    f
}

/// ASCII container: `[00][len][text][00]` records, optional FF FF terminator.
fn ascii_container(records: &[&str], terminator: bool) -> Vec<u8> {
    let mut f = header(MAGIC_ASCII, 0, 0);
    for t in records {
        f.push(0);
        f.push(t.len() as u8);
        f.extend_from_slice(t.as_bytes());
        f.push(0);
    }
    if terminator {
        f.extend_from_slice(&[0xFF, 0xFF]);
    }
    f
}

// ===================== assertion helpers =====================

fn emit(parsed: &ParsedFile) -> String {
    emit_source(parsed, &ConvertOptions::default())
}

fn warn_has(parsed: &ParsedFile, needle: &str) -> bool {
    parsed.warnings.iter().any(|w| w.message.contains(needle))
}

fn statements_of(line: &DecodedLine) -> &[Vec<Token>] {
    match line {
        DecodedLine::Tokens { statements, .. } => statements,
        DecodedLine::Source { .. } => panic!("expected a tokenized line"),
    }
}

fn geometry_name(parsed: &ParsedFile, idx: usize) -> String {
    format!("{:?}", parsed.sections[idx].geometry)
}

fn unique_temp_dir(tag: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!(
        "htbasic_conv_{tag}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

// ===================== header / dispatch =====================

#[test]
fn too_short_input_rejected() {
    assert!(matches!(
        decode(&[1, 2, 3]),
        Err(ConvertError::TooShort { needed: HEADER_LEN, .. })
    ));
}

#[test]
fn unknown_magic_rejected() {
    let f = header(0x1234, 0x08, 0);
    assert!(matches!(
        decode(&f),
        Err(ConvertError::NotAContainer { magic: 0x1234 })
    ));
}

#[test]
fn keyboard_container_rejected() {
    // 87 84 = keyboard/install files, not programs.
    let f = header(MAGIC_OTHER, 0x08, 0);
    assert!(matches!(
        decode(&f),
        Err(ConvertError::UnsupportedContainer { magic: MAGIC_OTHER })
    ));
}

#[test]
fn data_len_field_falls_back_to_eof() {
    let sections = [section(1, 0x99, &[], &[rec_modern(10, 8, &[&[0x33]], 0xFF)], &[])];
    let data: Vec<u8> = sections.concat();

    // declared = 0 → EOF fallback
    let mut f0 = header(MAGIC_TOKENIZED, 0x08, 0);
    f0.extend_from_slice(&data);
    // declared = exact length
    let mut f1 = header(MAGIC_TOKENIZED, 0x08, data.len() as u32);
    f1.extend_from_slice(&data);
    // declared = bogus (past EOF) → EOF fallback
    let mut f2 = header(MAGIC_TOKENIZED, 0x08, 0xFFFF);
    f2.extend_from_slice(&data);

    let e0 = emit(&decode(&f0).unwrap());
    assert_eq!(emit(&decode(&f1).unwrap()), e0);
    assert_eq!(emit(&decode(&f2).unwrap()), e0);
    assert_eq!(e0, "10        END\n");
}

// ===================== ASCII containers (88 84) =====================

#[test]
fn ascii_round_trip() {
    let f = ascii_container(&["10 PRINT \"HI\"", "20 END"], true);
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.kind, ContainerKind::Ascii);
    assert_eq!(parsed.variant, None);
    assert!(parsed.warnings.is_empty());
    assert_eq!(emit(&parsed), "10 PRINT \"HI\"\n20 END\n");
}

#[test]
fn ascii_stray_zeros_are_silent() {
    // 00 prefixes/suffixes are indistinguishable from padding — consumed
    // without warning; extra 00 before the terminator too.
    let mut f = header(MAGIC_ASCII, 0, 0);
    f.extend_from_slice(&[0, 0, 2, b'1', b'0', 0, 0, 0, 2, b'2', b'0', 0, 0, 0xFF, 0xFF]);
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(emit(&parsed), "10\n20\n");
}

#[test]
fn ascii_truncated_record_warns() {
    let mut f = header(MAGIC_ASCII, 0, 0);
    // len byte claims 64 bytes, only 2 present.
    f.extend_from_slice(&[0, 0x40, b'1', b'0']);
    let parsed = decode(&f).unwrap();
    assert!(warn_has(&parsed, "truncated ASCII record"));
    assert!(parsed.sections[0].lines.is_empty());
}

#[test]
fn ascii_ff_ff_terminator_stops_the_walk() {
    // Anything after FF FF is ignored.
    let mut f = ascii_container(&["10 PRINT A"], true);
    f.extend_from_slice(&[0, 9, b'j', b'u', b'n', b'k', b' ', b'x', b'x', b'x', 0]);
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(emit(&parsed), "10 PRINT A\n");
}

#[test]
fn ascii_line_number_split() {
    let f = ascii_container(&["10 PRINT A", "99", "no number here"], true);
    let parsed = decode(&f).unwrap();
    let lines: Vec<(u32, String)> = parsed.sections[0]
        .lines
        .iter()
        .map(|l| match l {
            DecodedLine::Source { number, text } => (*number, text.clone()),
            DecodedLine::Tokens { .. } => panic!("ASCII containers decode to Source lines"),
        })
        .collect();
    assert_eq!(
        lines,
        vec![
            (10, " PRINT A".to_string()),
            (99, String::new()),
            (0, "no number here".to_string()),
        ]
    );
    // Emitter writes Source lines as number + text with no separator, and
    // always includes the number (0 for lines without one).
    assert_eq!(emit(&parsed), "10 PRINT A\n99\n0no number here\n");
}

// ===================== tokenized structure / walker =====================

#[test]
fn tokenized_basic_section_decodes() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &["Finish", "X"],
            &[rec_modern(10, 8, &[&[0x33]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.kind, ContainerKind::Tokenized);
    assert_eq!(parsed.variant, Some(0x08));
    assert!(parsed.warnings.is_empty());
    assert!(parsed.unknown_opcodes.is_empty());

    let sec = &parsed.sections[0];
    assert_eq!(sec.stype, 1);
    assert_eq!(sec.name_table, vec!["Finish".to_string(), "X".to_string()]);
    assert_eq!(geometry_name(&parsed, 0), "Modern");

    let line = &sec.lines[0];
    match line {
        DecodedLine::Tokens {
            number,
            indent,
            flag,
            statements,
        } => {
            assert_eq!(*number, 10);
            assert_eq!(*indent, 0x0700); // 8 spaces − 1, high byte
            assert_eq!(*flag, 0xFF);
            assert_eq!(statements, &vec![vec![Token::Kw("END")]]);
        },
        DecodedLine::Source { .. } => panic!("expected Tokens line"),
    }
    assert_eq!(emit(&parsed), "10        END\n");
}

#[test]
fn multi_section_main_and_sub() {
    let f = tokenized(
        0x08,
        &[
            section(1, 0x99, &[], &[rec_modern(10, 8, &[&[0x33]], 0xFF)], &[]),
            section(
                2,
                0x24,
                &["Prtmat"],
                &[
                    rec_modern(100, 8, &[&[0xA6, 0xC7, 0x00]], 0xFF),
                    rec_modern(110, 8, &[&[0xA4]], 0xFF),
                ],
                &[],
            ),
        ],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(parsed.sections.len(), 2);
    assert_eq!(parsed.sections[0].stype, 1);
    assert_eq!(parsed.sections[1].stype, 2);
    assert_eq!(
        emit(&parsed),
        "10        END\n\n100        SUB Prtmat\n110        SUBEND\n"
    );
}

#[test]
fn sub_section_adaptive_preamble() {
    // SUB sections carry longer preambles; the record start must be found by
    // the adaptive scan, not a hardcoded offset.
    let f = tokenized(
        0x08,
        &[section(
            2,
            0x60,
            &["Prtmat"],
            &[rec_modern(100, 8, &[&[0xA6, 0xC7, 0x00]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(parsed.sections[0].name_table, vec!["Prtmat".to_string()]);
    assert_eq!(parsed.sections[0].lines.len(), 1);
    assert_eq!(emit(&parsed), "100        SUB Prtmat\n");
}

#[test]
fn tail_between_sections_consumed() {
    let sec1 = section(1, 0x99, &[], &[rec_modern(10, 8, &[&[0x33]], 0xFF)], &[]);
    // Second record keeps the section above the 0x30 minimum slen; it is
    // empty and the emitter skips statement-less lines.
    let sec2 = section(
        2,
        0x24,
        &[],
        &[
            rec_modern(100, 8, &[&[0xA4]], 0xFF),
            rec_modern(110, 8, &[], 0xFF),
        ],
        &[],
    );
    let mut f = header(MAGIC_TOKENIZED, 0x08, 0);
    f.extend_from_slice(&sec1);
    f.extend_from_slice(&[0x7F, 0x00, 0x00]);
    f.extend_from_slice(&sec2);
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.sections.len(), 2);
    assert!(warn_has(&parsed, "stray section terminator between sections"));
    assert_eq!(emit(&parsed), "10        END\n\n100        SUBEND\n");
}

#[test]
fn tail_inside_section_stops_the_walk() {
    // 7f 00 00 inside slen after the last record: the walker stops at it.
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(10, 8, &[&[0x33]], 0xFF)],
            &[0x7F, 0x00, 0x00],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.sections[0].lines.len(), 1);
    assert!(warn_has(&parsed, "end-of-section terminator tail"));
    assert_eq!(emit(&parsed), "10        END\n");
}

#[test]
fn bad_slen_rejected() {
    // slen below the minimum
    let mut f = header(MAGIC_TOKENIZED, 0x08, 0);
    f.extend_from_slice(&[1, 0x10]);
    f.extend_from_slice(&0x10u32.to_le_bytes());
    f.extend_from_slice(&[0xAB; 0x10]);
    assert!(matches!(
        decode(&f),
        Err(ConvertError::BadSectionHeader { offset: HEADER_LEN })
    ));

    // slen running past the data end
    let mut f = header(MAGIC_TOKENIZED, 0x08, 0);
    f.extend_from_slice(&[1, 0x10]);
    f.extend_from_slice(&0x50u32.to_le_bytes());
    f.extend_from_slice(&[0xAB; 0x30]);
    assert!(matches!(
        decode(&f),
        Err(ConvertError::BadSectionHeader { offset: HEADER_LEN })
    ));
}

#[test]
fn stray_zero_between_records_tolerated() {
    let rec1 = rec_modern(10, 8, &[&[0x33]], 0xFF);
    let rec2 = rec_modern(20, 8, &[&[0xA2]], 0xFF);
    let tail = [&[0x00][..], &chain_from(&[rec2], rec1[0])].concat();
    let f = tokenized(0x08, &[section(1, 0x99, &[], &[rec1], &tail)]);
    let parsed = decode(&f).unwrap();
    assert!(warn_has(&parsed, "stray 0x00 between line records"));
    assert_eq!(parsed.sections[0].lines.len(), 2);
    assert_eq!(emit(&parsed), "10        END\n20        STOP\n");
}

#[test]
fn three_stray_zeros_still_walk_through() {
    // Each walker iteration skips at most two stray zeros (one warning per
    // run); a third zero starts another skip run, so the record after it is
    // still decoded.
    let rec1 = rec_modern(10, 8, &[&[0x33]], 0xFF);
    let rec2 = rec_modern(20, 8, &[&[0xA2]], 0xFF);
    let tail = [&[0x00, 0x00, 0x00][..], &chain_from(&[rec2], rec1[0])].concat();
    let f = tokenized(0x08, &[section(1, 0x99, &[], &[rec1], &tail)]);
    let parsed = decode(&f).unwrap();
    assert!(warn_has(&parsed, "stray 0x00 between line records"));
    assert_eq!(parsed.sections[0].lines.len(), 2);
    assert_eq!(emit(&parsed), "10        END\n20        STOP\n");
}

#[test]
fn truncated_final_record_decodes_partially() {
    // A final record whose declared length runs past the section end (real
    // files do this on SUBEND) exposes only its header + body.
    let rec1 = rec_modern(10, 8, &[&[0x33]], 0xFF);
    let stub = [100, rec1[0], 20, 0, 0, 7, 0x33]; // declared len 100, 7 present
    let f = tokenized(0x08, &[section(1, 0x99, &[], &[rec1], &stub)]);
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.sections[0].lines.len(), 2);
    let stub_line = &parsed.sections[0].lines[1];
    match stub_line {
        DecodedLine::Tokens {
            number,
            flag,
            statements,
            ..
        } => {
            assert_eq!(*number, 20);
            assert_eq!(*flag, 0xFF); // truncated records report flag FF
            assert_eq!(statements, &vec![vec![Token::Kw("END")]]);
        },
        DecodedLine::Source { .. } => panic!("expected Tokens line"),
    }
    assert_eq!(emit(&parsed), "10        END\n20        END\n");
}

#[test]
fn empty_name_table_falls_back() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(10, 8, &[&[0x7F, 0xC7, 0x00]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(warn_has(&parsed, "variable-name index 0 out of range"));
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Kw("PRINT"), Token::Var("Uv00".into())]]
    );
    assert_eq!(emit(&parsed), "10        PRINT Uv00\n");
}

#[test]
fn label_definition_and_reference_via_c8() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &["Finish"],
            &[
                rec_modern(80, 8, &[&[0xC8, 0x00, 0x00]], 0xFF),
                rec_modern(90, 8, &[&[0x42, 0xC8, 0x00, 0x00]], 0xFF),
            ],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::LabelDef("Finish".into())]]
    );
    assert_eq!(
        statements_of(&parsed.sections[0].lines[1]),
        &[vec![Token::Kw("GOTO"), Token::LabelRef("Finish".into())]]
    );
    assert_eq!(emit(&parsed), "80        Finish:\n90        GOTO Finish\n");
}

#[test]
fn call_name_index_is_one_based() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &["Prtmat"],
            &[rec_modern(10, 8, &[&[0x0C, 0x0B, 0xC7, 0x01]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Kw("CALL"), Token::Var("Prtmat".into())]]
    );
    assert_eq!(emit(&parsed), "10        CALL Prtmat\n");
}

// ===================== token decoding =====================

#[test]
fn literal_tokens() {
    let mut real = vec![0xD3, 0x41];
    real.extend_from_slice(&3.14f64.to_le_bytes());
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(
                10,
                8,
                &[
                    &[0xCB, 0x05],
                    &[0xCC, 0x05, 0x00],
                    &[0xCD, 0xE8, 0x03],
                    &real,
                    &[0xCF, 0x03, b'a', b'b', b'c'],
                    &[0xFF, 0x70, 0x80, 0x51, 0x01, 0x00],
                ],
                0xFF,
            )],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[
            vec![Token::Int(5)],
            vec![Token::Int(5)],
            vec![Token::Int(1000)],
            vec![Token::Real(3.14)],
            vec![Token::Str("abc".into())],
            vec![Token::Int(86400)],
        ]
    );
    assert_eq!(
        emit(&parsed),
        "10        5 : 5 : 1000 : 3.14 : \"abc\" : 86400\n"
    );
}

#[test]
fn print_with_tab_chrx_round_trips() {
    let mut print_stmt = vec![0x7F, 0xCF, 0x05];
    print_stmt.extend_from_slice(b"[TAB]");
    print_stmt.extend_from_slice(&[0xE6, 0xDB, 0xE0, 0xCB, 0x0F, 0xE1, 0xE6, 0xCF, 0x09]);
    print_stmt.extend_from_slice(b"15 spaces");

    let chrx = vec![0x7F, 0xF7, 0xE0, 0xCB, 0x84, 0xE1];
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(
                20,
                8,
                &[&print_stmt, &chrx],
                0xFF,
            )],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[
            vec![
                Token::Kw("PRINT"),
                Token::Str("[TAB]".into()),
                Token::Punct(";"),
                Token::Fn("TAB"),
                Token::Punct("("),
                Token::Int(15),
                Token::Punct(")"),
                Token::Punct(";"),
                Token::Str("15 spaces".into()),
            ],
            vec![
                Token::Kw("PRINT"),
                Token::Fn("CHR$"),
                Token::Punct("("),
                Token::Int(132),
                Token::Punct(")"),
            ],
        ]
    );
    assert_eq!(
        emit(&parsed),
        // `;` is Sp::Close — glued to what follows, as in the original
        // sources: PRINT "[TAB]";TAB(15);"15 spaces".
        "20        PRINT \"[TAB]\"; TAB(15);\"15 spaces\" : PRINT CHR$(132)\n"
    );
}

#[test]
fn option_base_compound_token() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(10, 8, &[&[0x6D, 0xFF, 0x93, 0xCB, 0x00]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Kw("OPTION BASE"), Token::Int(0)]]
    );
    assert_eq!(emit(&parsed), "10        OPTION BASE 0\n");
}

#[test]
fn comment_token_emission() {
    let mut comment = vec![0x01, 0x02, 0x0B];
    comment.extend_from_slice(b"This is a comment");
    // spaces = 0 → comment lines render with a single space
    let f = tokenized(
        0x08,
        &[section(1, 0x99, &[], &[rec_modern(10, 0, &[&comment], 0xFF)], &[])],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Comment("This is a comment".into())]]
    );
    assert_eq!(emit(&parsed), "10 !This is a comment\n");
}

#[test]
fn unknown_statement_degrades_to_comment() {
    // 0xA1 is unmapped and non-printable → whole statement becomes a comment.
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(10, 8, &[&[0xA1, 0xCB, 0x05]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.unknown_opcodes.get("0xA1"), Some(&1));
    assert!(warn_has(&parsed, "unknown opcode 0xA1"));
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::UnknownStmt(vec![0xA1, 0xCB, 0x05])]]
    );
    assert_eq!(emit(&parsed), "10        ! U A1 CB 05\n");
}

#[test]
fn unknown_mid_expression_placeholder() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(10, 8, &[&[0x7F, 0xA1, 0xCB, 0x05]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.unknown_opcodes.get("0xA1"), Some(&1));
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![
            Token::Kw("PRINT"),
            Token::Unknown(vec![0xA1]),
            Token::Int(5),
        ]]
    );
    assert_eq!(emit(&parsed), "10        PRINT UhA1 5\n");
}

#[test]
fn unknown_ff_pair_warns() {
    let f = tokenized(
        0x08,
        &[section(1, 0x99, &[], &[rec_modern(10, 8, &[&[0xFF, 0xCF]], 0xFF)], &[])],
    );
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.unknown_opcodes.get("0xFF"), Some(&1));
    assert_eq!(parsed.unknown_opcodes.get("0xCF"), Some(&1));
    assert!(warn_has(&parsed, "unknown multi-byte opcode FF CF"));
    assert_eq!(emit(&parsed), "10        ! U FF CF\n");
}

#[test]
fn dll_comment_out_and_off() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &["Finish"],
            &[rec_modern(10, 8, &[&[0xBF, 0x42, 0xC8, 0x00, 0x00]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Dll(vec![0xBF, 0x42, 0xC8, 0x00, 0x00])]]
    );

    // Default: DLL stays a comment.
    assert_eq!(emit(&parsed), "10        ! DLL 42 C8 00 00\n");
    // comment_out_dll=false re-decodes the body (here: GOTO <label>).
    let raw = emit_source(
        &parsed,
        &ConvertOptions {
            comment_out_dll: false,
            ..ConvertOptions::default()
        },
    );
    assert_eq!(raw, "10        GOTO Finish\n");
}

#[test]
fn ca_operand_terminator_rules() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            // Pad names keep every referenced index ≥ 2 so no C7/C8 index
            // byte is 0x00 or 0x01 — a 0x01 byte anywhere in the body is
            // misread as a comment token by the statement splitter.
            &["XX", "YY", "Finish", "Here"],
            &[rec_modern(
                10,
                8,
                &[
                    &[0x42, 0xC8, 0x02, 0x00, 0xCA],           // suppressed at end
                    &[0x42, 0xC8, 0x02, 0x00, 0xCA, 0xCB, 0x05], // comma before a value
                    &[0x6B, 0xFF, 0xA1, 0xCB, 0x05, 0xCA, 0x42, 0xC7, 0x03], // before GOTO
                    &[0xCA, 0xE1], // suppressed before )
                    &[0xCA, 0xCB, 0x05], // comma before a value
                ],
                0xFF,
            )],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(
        emit(&parsed),
        "10        GOTO Finish : GOTO Finish,5 : ON CYCLE 5 GOTO Here : ) : ,5\n"
    );
}

#[test]
fn mat_forms_decode() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            // Padded so every referenced index is ≥ 2: a 0x01 index byte
            // would be misread as a comment token by the statement splitter.
            &["XX", "YY", "A", "B", "V"],
            &[rec_modern(
                10,
                8,
                &[
                    &[0x64, 0xFF, 0xE8, 0xC7, 0x02],
                    &[0x64, 0xFF, 0xE8, 0xC7, 0x02, 0xD2], // (*) suppressed in MAT
                    &[0x64, 0xFF, 0xE8, 0xC7, 0x02, 0xD2, 0xFF, 0xA6],
                    &[0x64, 0xFF, 0xE8, 0xC7, 0x02, 0xD4, 0xC7, 0x03],
                    &[0x64, 0xFF, 0xE0, 0xC7, 0x02, 0xFF, 0x98, 0xC7, 0x04, 0xDE, 0xCB, 0x01],
                ],
                0xFF,
            )],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    let stmts = statements_of(&parsed.sections[0].lines[0]);
    assert_eq!(
        stmts,
        &[
            vec![Token::Kw("MAT"), Token::Kw("SORT"), Token::Var("A".into())],
            vec![Token::Kw("MAT"), Token::Kw("SORT"), Token::Var("A".into())],
            vec![
                Token::Kw("MAT"),
                Token::Kw("SORT"),
                Token::Var("A".into()),
                Token::Kw("DESC"),
            ],
            vec![
                Token::Kw("MAT"),
                Token::Kw("SORT"),
                Token::Var("A".into()),
                Token::Kw("TO"),
                Token::Var("B".into()),
            ],
            vec![
                Token::Kw("MAT"),
                Token::Kw("REORDER"),
                Token::Var("A".into()),
                Token::Kw("BY"),
                Token::Var("V".into()),
                Token::Punct(","),
                Token::Int(1),
            ],
        ]
    );
    assert_eq!(
        emit(&parsed),
        "10        MAT SORT A : MAT SORT A : MAT SORT A DESC : MAT SORT A TO B : MAT REORDER A BY V,1\n"
    );
}

#[test]
fn def_fn_header_and_fnend() {
    let f = tokenized(
        0x08,
        &[
            // "Pad" lengthens the table past 4 bytes — parse_name_table
            // never tries a start within 4 of the preamble end, so a lone
            // ["Add"] table ending exactly at the record start is invisible.
            section(
                1,
                0x99,
                &["Add", "Pad"],
                &[rec_modern(10, 8, &[&[0x7F, 0xF8, 0xC7, 0x00, 0xE0, 0xE1]], 0xFF)],
                &[],
            ),
            section(
                3,
                0x24,
                &["Add", "Pad"],
                &[
                    rec_modern(100, 8, &[&[0x24, 0xF8, 0xC7, 0x00, 0xE0, 0xE1]], 0xFF),
                    rec_150(0xFF),
                ],
                &[],
            ),
        ],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty(), "warnings: {:?}", parsed.warnings);
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![
            Token::Kw("PRINT"),
            Token::FnCall("Add".into()),
            Token::Punct("("),
            Token::Punct(")"),
        ]]
    );
    assert_eq!(
        statements_of(&parsed.sections[1].lines[0]),
        &[vec![
            Token::Kw("DEF FN"),
            Token::Var("Add".into()),
            Token::Punct("("),
            Token::Punct(")"),
        ]]
    );
    assert_eq!(
        statements_of(&parsed.sections[1].lines[1]),
        &[vec![Token::Kw("FNEND")]]
    );
    assert_eq!(
        emit(&parsed),
        "10        PRINT FNAdd()\n\n100        DEF FN Add()\n150        FNEND\n"
    );
}

/// `150 FNEND` — a line whose body is the single 0x39 byte.
fn rec_150(flag: u8) -> Vec<u8> {
    rec_modern(150, 8, &[&[0x39]], flag)
}

#[test]
fn old_geometry_records_and_dialect_keyword() {
    // Old geometry: X precedes the line number; X is the space count itself.
    // 0x72 is CLEAR in the old dialect, PAUSE in the modern one.
    let f_old = tokenized(
        0x04,
        &[section(
            1,
            0x99,
            &[],
            &[rec_old(10, 8, &[&[0x72]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f_old).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(geometry_name(&parsed, 0), "Old");
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Kw("CLEAR")]]
    );
    assert_eq!(emit(&parsed), "10        CLEAR\n");

    let f_modern = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[rec_modern(10, 8, &[&[0x72]], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f_modern).unwrap();
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Kw("PAUSE")]]
    );
}

#[test]
fn old_dialect_comment_b_off_by_one() {
    // The comment length byte is unreliable in old files (text+1 seen);
    // the decode takes the rest of the statement and clamps non-printables.
    let mut comment = vec![0x01, 0x02, 0x05];
    comment.extend_from_slice(b"Hi");
    comment.extend_from_slice(&[0x00, 0x01]);
    let f = tokenized(
        0x04,
        &[section(
            1,
            0x99,
            &[],
            &[rec_old(10, 1, &[&comment], 0xFF)],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Comment("Hi".into())]]
    );
    assert_eq!(emit(&parsed), "10 !Hi\n");
}

#[test]
fn double_c9_and_empty_statement_handling() {
    // An empty statement between two C9s is dropped; two real statements are
    // joined with " : ". A record with no statements emits no line.
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[
                rec_modern(10, 8, &[&[0x33], &[], &[0xA2]], 0xFF),
                rec_modern(20, 8, &[], 0xFF), // entirely empty record
                rec_modern(30, 8, &[&[0xA2]], 0xFF),
            ],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    assert_eq!(parsed.sections[0].lines.len(), 3);
    assert_eq!(
        statements_of(&parsed.sections[0].lines[0]),
        &[vec![Token::Kw("END")], vec![Token::Kw("STOP")]]
    );
    assert_eq!(
        statements_of(&parsed.sections[0].lines[1]),
        &[] as &[Vec<Token>]
    );
    assert_eq!(emit(&parsed), "10        END : STOP\n30        STOP\n");
}

// ===================== emission =====================

#[test]
fn exact_full_file_emission() {
    let mut print_stmt = vec![0x7F, 0xCF, 0x05];
    print_stmt.extend_from_slice(b"[TAB]");
    print_stmt.extend_from_slice(&[0xE6, 0xDB, 0xE0, 0xCB, 0x0F, 0xE1, 0xE6, 0xCF, 0x09]);
    print_stmt.extend_from_slice(b"15 spaces");

    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &["Finish"],
            &[
                rec_modern(10, 8, &[&[0x15, 0xFF, 0xE3]], 0xFF),
                rec_modern(20, 8, &[&print_stmt], 0xFF),
                rec_modern(30, 8, &[&[0x42, 0xC8, 0x00, 0x00]], 0xFF),
                rec_modern(80, 8, &[&[0xC8, 0x00, 0x00]], 0xFF),
            ],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty(), "warnings: {:?}", parsed.warnings);
    assert_eq!(
        emit(&parsed),
        "10        CLEAR SCREEN\n\
         20        PRINT \"[TAB]\"; TAB(15);\"15 spaces\"\n\
         30        GOTO Finish\n\
         80        Finish:\n"
    );
}

#[test]
fn flag_values_are_preserved() {
    let f = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &[],
            &[
                rec_modern(10, 8, &[&[0x33]], 0x08),
                rec_modern(20, 8, &[&[0xA2]], 0x02),
                rec_modern(30, 8, &[&[0xA2]], 0xFF),
            ],
            &[],
        )],
    );
    let parsed = decode(&f).unwrap();
    assert!(parsed.warnings.is_empty());
    let flags: Vec<u8> = parsed.sections[0]
        .lines
        .iter()
        .map(|l| match l {
            DecodedLine::Tokens { flag, .. } => *flag,
            DecodedLine::Source { .. } => panic!("expected Tokens line"),
        })
        .collect();
    assert_eq!(flags, vec![0x08, 0x02, 0xFF]);
}

// ===================== robustness =====================

#[test]
fn garbage_section_never_panics() {
    let mut f = header(MAGIC_TOKENIZED, 0x08, 0);
    f.extend_from_slice(&[1, 0x10]);
    f.extend_from_slice(&0x30u32.to_le_bytes());
    f.extend_from_slice(&[0xAB; 0x2A]);
    let parsed = decode(&f).unwrap();
    assert_eq!(parsed.sections.len(), 1);
    assert!(parsed.sections[0].lines.is_empty());
    assert_eq!(emit(&parsed), "");
}

// ===================== CLI (spawns the built binary) =====================

#[test]
fn cli_convert_writes_output_and_check_parses_it() {
    let dir = unique_temp_dir("cli_convert");
    let input = dir.join("synth.prg");
    let output = dir.join("out.bas");
    let file = tokenized(
        0x08,
        &[section(
            1,
            0x99,
            &["Finish"],
            &[
                rec_modern(10, 8, &[&[0x15, 0xFF, 0xE3]], 0xFF),
                rec_modern(20, 8, &[&[0x7F, 0xCB, 0x05]], 0xFF),
                rec_modern(30, 8, &[&[0x33]], 0xFF),
            ],
            &[],
        )],
    );
    fs::write(&input, &file).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_htbasic"))
        .args(["-c", input.to_str().unwrap(), "-o", output.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());

    let parsed = decode(&file).unwrap();
    let expected = emit_source(&parsed, &ConvertOptions::default());
    assert_eq!(fs::read_to_string(&output).unwrap(), expected);
    assert_eq!(expected, "10        CLEAR SCREEN\n20        PRINT 5\n30        END\n");

    // `--check` decodes its input as a container and parse-checks the
    // emitted source — so it must receive the .prg, not the converted .bas.
    let status = Command::new(env!("CARGO_BIN_EXE_htbasic"))
        .args(["--check", input.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cli_refuses_to_overwrite_input() {
    // Default output is <stem>.bas next to the input — for an ASCII
    // container already named .bas that would overwrite the input.
    let dir = unique_temp_dir("cli_refuse");
    let input = dir.join("prog.bas");
    fs::write(&input, ascii_container(&["10 PRINT \"X\"", "20 END"], true)).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_htbasic"))
        .args(["-c", input.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(!status.success());
    let _ = fs::remove_dir_all(&dir);
}

// ===================== ground-truth pairs (feature-gated) =====================
// Dev machine only: reads the copyrighted TransEra distribution OUTSIDE the
// repo; the files themselves are never committed. `#[ignore]`d so they never
// run in normal test passes or CI.

#[cfg(feature = "htbwin-fixtures")]
mod ground_truth {
    use super::{decode, emit_source, ConvertOptions};
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn htbwin_dir() -> Option<PathBuf> {
        if let Ok(d) = std::env::var("HTBWIN_DIR") {
            let p = PathBuf::from(d);
            if p.is_dir() {
                return Some(p);
            }
        }
        let d = PathBuf::from(r"C:\Program Files (x86)\HTBwin95");
        d.is_dir().then_some(d)
    }

    /// Whitespace-stripped line set — the original .bas and the decoded .prg
    /// differ only in layout, so compare structure.
    fn stripped_set(src: &str) -> BTreeSet<String> {
        src.lines()
            .map(|l| l.chars().filter(|c| !c.is_whitespace()).collect())
            .filter(|s: &String| !s.is_empty())
            .collect()
    }

    fn check_pair(prg: &Path, bas: &Path) {
        let prg_bytes = fs::read(prg).unwrap_or_else(|e| panic!("read {}: {e}", prg.display()));
        let bas_bytes = fs::read(bas).unwrap_or_else(|e| panic!("read {}: {e}", bas.display()));
        let decoded = decode(&prg_bytes).unwrap_or_else(|e| panic!("decode {}: {e}", prg.display()));
        let ground = decode(&bas_bytes).unwrap_or_else(|e| panic!("decode {}: {e}", bas.display()));
        // The original .bas contains real `DLL LOAD …` statements, so compare
        // against the un-commented rendering of the tokenized side.
        let out = emit_source(
            &decoded,
            &ConvertOptions {
                comment_out_dll: false,
                ..ConvertOptions::default()
            },
        );
        let gt = emit_source(&ground, &ConvertOptions::default());
        let out_set = stripped_set(&out);
        let gt_set = stripped_set(&gt);
        let missing: Vec<&String> = gt_set.difference(&out_set).collect();
        assert!(
            missing.is_empty(),
            "{} ground-truth line(s) not produced by decode (first 10 shown):\n  {}\nDecoded output:\n{out}",
            missing.len(),
            missing
                .iter()
                .take(10)
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n  "),
        );
    }

    #[test]
    #[ignore = "dev-machine only: reads copyrighted TransEra files (never committed)"]
    fn print_pair_matches() {
        let Some(dir) = htbwin_dir() else {
            eprintln!("HTBwin95 not found; skipping ground-truth test");
            return;
        };
        check_pair(
            &dir.join(r"examples\print.prg"),
            &dir.join(r"examples\print.bas"),
        );
    }

    #[test]
    #[ignore = "dev-machine only: reads copyrighted TransEra files (never committed)"]
    fn htbclipboard_pair_matches() {
        let Some(dir) = htbwin_dir() else {
            eprintln!("HTBwin95 not found; skipping ground-truth test");
            return;
        };
        check_pair(
            &dir.join(r"DLL Toolkit\Runtime Samples\HTBClipboard\HTBClipboard.prg"),
            &dir.join(r"DLL Toolkit\Samples\HTBClipboard\HTBClipboard.bas"),
        );
    }
}
