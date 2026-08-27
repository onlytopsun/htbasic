//! Per-file dialect detection: line-record geometry and token-table flavor.

/// Line-record geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geometry {
    /// `[len][prev][u16 linenum][u16 X][body][C9][flag]` — the common `08 00` variant.
    Modern,
    /// `[len][prev][u8 X][u16 linenum][body][C9][flag]` — the older `04 00` variant.
    Old,
}

#[derive(Debug, Clone, Copy)]
pub struct Dialect {
    /// First two preamble bytes (02 00 / 00 00 / 03 00 / 06 00).
    pub marker: [u8; 2],
    pub geometry: Geometry,
}

impl Dialect {
    pub fn detect(variant: u8, marker: [u8; 2]) -> Self {
        let geometry = if variant == 0x04 {
            Geometry::Old
        } else {
            Geometry::Modern
        };
        Self { marker, geometry }
    }
}
