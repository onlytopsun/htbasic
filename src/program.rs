/// Program buffer for the HTBasic REPL.
///
/// Stores source lines keyed by line number (BTreeMap for ordered iteration).
/// Supports: insert, replace, delete, LIST with ranges, REN, SAVE, LOAD.
use std::collections::BTreeMap;
use std::fs;

/// A single program line: line number + source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramLine {
    pub number: u32,
    pub text: String,
}

/// The program buffer. Lines are stored sorted by number.
#[derive(Debug, Clone, Default)]
pub struct ProgramBuffer {
    lines: BTreeMap<u32, String>,
    /// Free-form lines before END (labels, unnumbered statements) — stored for LOAD/SAVE
    header_lines: Vec<String>,
}

impl ProgramBuffer {
    pub fn new() -> Self {
        Self {
            lines: BTreeMap::new(),
            header_lines: Vec::new(),
        }
    }

    /// Insert or replace a line. Returns the old text if replaced.
    pub fn put(&mut self, number: u32, text: &str) -> Option<String> {
        self.lines.insert(number, text.to_string())
    }

    /// Delete a line by number. Returns the text if it existed.
    pub fn delete(&mut self, number: u32) -> Option<String> {
        self.lines.remove(&number)
    }

    /// Get a line by number.
    pub fn get(&self, number: u32) -> Option<&str> {
        self.lines.get(&number).map(|s| s.as_str())
    }

    /// Check if the program is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Number of lines.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Get all lines as a Vec, sorted by line number.
    pub fn to_vec(&self) -> Vec<ProgramLine> {
        self.lines
            .iter()
            .map(|(&number, text)| ProgramLine {
                number,
                text: text.clone(),
            })
            .collect()
    }

    /// Get the full source text (for compilation).
    pub fn to_source(&self) -> String {
        let mut source = String::new();
        for line in self.header_lines.iter() {
            source.push_str(line);
            source.push('\n');
        }
        for (num, text) in &self.lines {
            source.push_str(&format!("{} {}\n", num, text));
        }
        source
    }

    /// Get source text without END marker (for REPL immediate compilation).
    pub fn to_source_no_end(&self) -> String {
        let mut source = String::new();
        for line in self.header_lines.iter() {
            source.push_str(line);
            source.push('\n');
        }
        for (num, text) in &self.lines {
            source.push_str(&format!("{} {}\n", num, text));
        }
        source
    }

    /// LIST — return formatted listing.
    pub fn list(&self, range: Option<(Option<u32>, Option<u32>)>) -> Vec<String> {
        let mut result = Vec::new();
        let (start, end) = range.unwrap_or((None, None));

        for (&num, text) in &self.lines {
            let show = match (start, end) {
                (None, None) => true,
                (Some(s), None) => num >= s,
                (None, Some(e)) => num <= e,
                (Some(s), Some(e)) => num >= s && num <= e,
            };
            if show {
                result.push(format!("{} {}", num, text));
            }
        }
        result
    }

    /// REN — renumber lines starting at `start` with `step` increment.
    pub fn renumber(&mut self, start: u32, step: u32) {
        let old_lines: Vec<(u32, String)> =
            self.lines.iter().map(|(&n, t)| (n, t.clone())).collect();
        self.lines.clear();
        for (i, (_old_num, text)) in old_lines.iter().enumerate() {
            let new_num = start + (i as u32) * step;
            self.lines.insert(new_num, text.clone());
        }
    }

    /// Delete a range of lines.
    pub fn delete_range(&mut self, range: (Option<u32>, Option<u32>)) -> usize {
        let to_delete: Vec<u32> = self
            .lines
            .keys()
            .filter(|&&n| {
                let ok_start = range.0.map(|s| n >= s).unwrap_or(true);
                let ok_end = range.1.map(|e| n <= e).unwrap_or(true);
                ok_start && ok_end
            })
            .copied()
            .collect();
        let count = to_delete.len();
        for n in to_delete {
            self.lines.remove(&n);
        }
        count
    }

    /// Clear all lines.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.header_lines.clear();
    }

    /// SAVE to ASCII .BAS file.
    pub fn save(&self, filename: &str) -> std::io::Result<()> {
        let source = self.to_source();
        fs::write(filename, &source)
    }

    /// SAVE to binary .HTB file (internal format with magic header).
    pub fn save_binary(&self, filename: &str) -> std::io::Result<()> {
        use std::io::Write;
        let source = self.to_source();
        let mut file = fs::File::create(filename)?;
        // Magic: HTB\0
        file.write_all(b"HTB\0")?;
        // Version
        file.write_all(&1u32.to_le_bytes())?;
        // Source length
        file.write_all(&(source.len() as u32).to_le_bytes())?;
        // Source text
        file.write_all(source.as_bytes())?;
        Ok(())
    }

    /// GET (load) from binary .HTB file.
    pub fn get_binary(&mut self, filename: &str) -> std::io::Result<usize> {
        use std::io::Read;
        let mut file = fs::File::open(filename)?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != b"HTB\0" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Not a valid HTBasic binary file",
            ));
        }
        let mut version_bytes = [0u8; 4];
        file.read_exact(&mut version_bytes)?;
        let _version = u32::from_le_bytes(version_bytes);
        let mut len_bytes = [0u8; 4];
        file.read_exact(&mut len_bytes)?;
        let len = u32::from_le_bytes(len_bytes) as usize;
        let mut source = vec![0u8; len];
        file.read_exact(&mut source)?;
        let source_str = String::from_utf8_lossy(&source).to_string();
        // Parse the source back into the buffer
        self.clear();
        self.load_from_source(&source_str)
    }

    /// Parse source text into the buffer (shared by LOAD and GET).
    fn load_from_source(&mut self, content: &str) -> std::io::Result<usize> {
        self.clear();
        let mut count = 0;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(space_pos) = trimmed.find(' ') {
                let num_part = &trimmed[..space_pos];
                if let Ok(num) = num_part.parse::<u32>() {
                    let text = trimmed[space_pos + 1..].to_string();
                    self.lines.insert(num, text);
                    count += 1;
                    continue;
                }
            }
            if let Ok(num) = trimmed.parse::<u32>() {
                self.lines.insert(num, String::new());
                count += 1;
                continue;
            }
            self.header_lines.push(trimmed.to_string());
        }
        Ok(count)
    }

    /// MERGE lines from a .BAS file into current buffer (don't clear first).
    pub fn merge(&mut self, filename: &str) -> std::io::Result<usize> {
        let content = fs::read_to_string(filename)?;
        let mut count = 0;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(space_pos) = trimmed.find(' ') {
                let num_part = &trimmed[..space_pos];
                if let Ok(num) = num_part.parse::<u32>() {
                    let text = trimmed[space_pos + 1..].to_string();
                    self.lines.insert(num, text);
                    count += 1;
                    continue;
                }
            }
        }
        Ok(count)
    }

    /// LOAD from .BAS file.
    pub fn load(&mut self, filename: &str) -> std::io::Result<usize> {
        let content = fs::read_to_string(filename)?;
        self.load_from_source(&content)
    }

    /// Parse user input: if it starts with a number, treat as program line.
    /// Returns None if not a line-number input, Some(number, text) if it is.
    pub fn parse_input(input: &str) -> Option<(u32, String)> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Check if input starts with a digit
        let first_char = trimmed.chars().next()?;
        if !first_char.is_ascii_digit() {
            return None;
        }
        // Find the end of the number
        let num_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        let num_str = &trimmed[..num_end];
        if let Ok(num) = num_str.parse::<u32>() {
            let text = trimmed[num_end..].trim().to_string();
            Some((num, text))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_input() {
        assert_eq!(
            ProgramBuffer::parse_input("10 PRINT X"),
            Some((10, "PRINT X".into()))
        );
        assert_eq!(ProgramBuffer::parse_input("100"), Some((100, "".into())));
        assert_eq!(ProgramBuffer::parse_input("PRINT X"), None);
        assert_eq!(ProgramBuffer::parse_input(""), None);
    }

    #[test]
    fn test_insert_delete() {
        let mut buf = ProgramBuffer::new();
        buf.put(10, "PRINT X");
        buf.put(20, "PRINT Y");
        assert_eq!(buf.get(10), Some("PRINT X"));
        buf.delete(10);
        assert_eq!(buf.get(10), None);
        assert_eq!(buf.get(20), Some("PRINT Y"));
    }

    #[test]
    fn test_list_range() {
        let mut buf = ProgramBuffer::new();
        buf.put(10, "PRINT 1");
        buf.put(20, "PRINT 2");
        buf.put(30, "PRINT 3");
        let lines = buf.list(Some((Some(15), Some(25))));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "20 PRINT 2");
    }

    #[test]
    fn test_renumber() {
        let mut buf = ProgramBuffer::new();
        buf.put(10, "PRINT A");
        buf.put(25, "GOTO 10");
        buf.renumber(100, 10);
        let lines = buf.to_vec();
        assert_eq!(lines[0].number, 100);
        assert_eq!(lines[1].number, 110);
    }
}
