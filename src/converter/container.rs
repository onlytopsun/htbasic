//! Container-level decoding: 256-byte typed-file header, magic dispatch,
//! ASCII-record walker, and tokenized section walker.

use super::{
    ContainerKind, ConvertError, ConvertWarning, DecodedLine, ParsedFile, Section, HEADER_LEN,
    MAGIC_ASCII, MAGIC_OTHER, MAGIC_TOKENIZED,
};
use std::collections::BTreeMap;

pub fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

pub fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn decode(bytes: &[u8]) -> Result<ParsedFile, ConvertError> {
    if bytes.len() < HEADER_LEN {
        return Err(ConvertError::TooShort {
            offset: bytes.len(),
            needed: HEADER_LEN,
        });
    }
    let magic = u16le(&bytes[0..2]);
    match magic {
        MAGIC_ASCII => decode_ascii(bytes),
        MAGIC_TOKENIZED => decode_tokenized(bytes),
        MAGIC_OTHER => Err(ConvertError::UnsupportedContainer { magic }),
        _ => Err(ConvertError::NotAContainer { magic }),
    }
}

/// Walk `88 84` ASCII records: `[00][len][verbatim source line][00]... FF FF`.
/// The 0x00 before and after each record are indistinguishable from stray
/// padding, so both are consumed silently; record boundaries are validated
/// by len bounds and the `FF FF` terminator instead.
fn decode_ascii(bytes: &[u8]) -> Result<ParsedFile, ConvertError> {
    let mut warnings = Vec::new();
    let mut lines = Vec::new();
    let mut pos = HEADER_LEN;
    while pos < bytes.len() {
        if pos + 1 < bytes.len() && bytes[pos] == 0xFF && bytes[pos + 1] == 0xFF {
            break;
        }
        if bytes[pos] == 0x00 {
            pos += 1; // record prefix
            continue;
        }
        let len = bytes[pos] as usize;
        pos += 1;
        if pos + len > bytes.len() {
            warnings.push(ConvertWarning {
                offset: pos - 1,
                message: "truncated ASCII record".into(),
            });
            break;
        }
        let text = String::from_utf8_lossy(&bytes[pos..pos + len]).into_owned();
        pos += len;
        // Optional trailing 0x00 record terminator.
        if pos < bytes.len() && bytes[pos] == 0x00 {
            pos += 1;
        }
        let (number, rest) = split_linenum(&text);
        lines.push(DecodedLine::Source {
            number,
            text: rest.to_string(),
        });
    }
    let section = Section {
        stype: 1,
        marker: [0, 0],
        geometry: super::dialect::Geometry::Modern,
        variant: 0,
        name_table: Vec::new(),
        imports: Vec::new(),
        lines,
    };
    Ok(ParsedFile {
        kind: ContainerKind::Ascii,
        variant: None,
        sections: vec![section],
        warnings,
        unknown_opcodes: BTreeMap::new(),
    })
}

/// Split `"<digits><rest>"`; rest keeps its original spacing.
fn split_linenum(text: &str) -> (u32, &str) {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return (0, text);
    }
    let number: u32 = text[..digits].parse().unwrap_or(0);
    (number, &text[digits..])
}

/// Walk `86 84` sections: `[type][0x10][u32 slen][preamble][records]`.
fn decode_tokenized(bytes: &[u8]) -> Result<ParsedFile, ConvertError> {
    let variant = bytes[0x10];
    let declared = u32le(&bytes[8..12]) as usize;
    // The length field is unreliable in some files — fall back to EOF.
    let data_end = if declared > 0 && HEADER_LEN + declared <= bytes.len() {
        HEADER_LEN + declared
    } else {
        bytes.len()
    };
    let mut warnings = Vec::new();
    let mut unknown: BTreeMap<String, usize> = BTreeMap::new();
    let mut sections = Vec::new();
    let mut main_imports: Vec<String> = Vec::new();
    let mut pos = HEADER_LEN;
    while pos + 6 <= data_end {
        // Tolerate stray padding / leftover terminators between sections.
        if bytes[pos] == 0x00 {
            pos += 1;
            warnings.push(ConvertWarning {
                offset: pos - 1,
                message: "stray 0x00 between sections".into(),
            });
            continue;
        }
        if bytes[pos] == 0x7F && pos + 3 <= data_end {
            pos += 3;
            warnings.push(ConvertWarning {
                offset: pos - 3,
                message: "stray section terminator between sections".into(),
            });
            continue;
        }
        let stype = bytes[pos];
        // 1 = main, 2 = SUB, 3 = DEF FN section (def fn.prg et al.).
        if !(1..=3).contains(&stype) || bytes[pos + 1] != 0x10 {
            return Err(ConvertError::BadSectionHeader { offset: pos });
        }
        let slen = u32le(&bytes[pos + 2..pos + 6]) as usize;
        if slen < 0x30 || pos + slen > data_end {
            return Err(ConvertError::BadSectionHeader { offset: pos });
        }
        // The section buffer is extended 8 bytes past `slen` (clamped to the
        // data end) so a final record truncated at the section boundary —
        // SUBEND records extend past their section — is still walkable;
        // decode_section receives the true `slen` and stays bounded by it.
        let sec_end = (pos + slen + 8).min(data_end);
        let section = super::section::decode_section(
            &bytes[pos..sec_end],
            slen,
            variant,
            &main_imports,
            &mut warnings,
            &mut unknown,
        );
        if section.stype == 1 {
            main_imports = section.imports.clone();
        }
        sections.push(section);
        pos += slen;
    }
    Ok(ParsedFile {
        kind: ContainerKind::Tokenized,
        variant: Some(variant),
        sections,
        warnings,
        unknown_opcodes: unknown,
    })
}
