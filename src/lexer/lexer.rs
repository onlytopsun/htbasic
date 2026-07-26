use crate::lexer::token::{Token, TokenKind};

/// Multi-word keyword lookup table.
/// When we encounter a word that could be the start of a multi-word keyword,
/// we check if the next word(s) complete it.
mod keywords {
    use super::TokenKind;

    /// Try to match a keyword (case-insensitive). Returns Some if matched.
    pub fn match_keyword(word: &str) -> Option<TokenKind> {
        match word.to_uppercase().as_str() {
            "DIM" => Some(TokenKind::Dim),
            "COM" => Some(TokenKind::Com),
            "REAL" => Some(TokenKind::Real),
            "INTEGER" => Some(TokenKind::Integer),
            "SHORT" => Some(TokenKind::Short),
            "LONG" => Some(TokenKind::Long),
            "COMPLEX" => Some(TokenKind::Complex),
            "ALLOCATE" => Some(TokenKind::Allocate),
            "DEALLOCATE" => Some(TokenKind::Deallocate),
            "REDIM" => Some(TokenKind::Redim),
            "STATIC" => Some(TokenKind::Static),
            "SUB" => Some(TokenKind::Sub),
            "SUBEND" => Some(TokenKind::SubEnd),
            "FN" => Some(TokenKind::DefFn), // DEF FN parsed specially
            "FNEND" => Some(TokenKind::FnEnd),
            "CALL" => Some(TokenKind::Call),
            "SUBEXIT" => Some(TokenKind::SubExit),
            "RETURN" => Some(TokenKind::Return),
            "LOADSUB" => Some(TokenKind::LoadSub),
            "DELSUB" => Some(TokenKind::DelSub),
            "IF" => Some(TokenKind::If),
            "THEN" => Some(TokenKind::Then),
            "ELSE" => Some(TokenKind::Else),
            "FOR" => Some(TokenKind::For),
            "TO" => Some(TokenKind::To),
            "STEP" => Some(TokenKind::Step),
            "NEXT" => Some(TokenKind::Next),
            "WHILE" => Some(TokenKind::While),
            "LOOP" => Some(TokenKind::Loop_),
            "REPEAT" => Some(TokenKind::Repeat),
            "UNTIL" => Some(TokenKind::Until),
            "SELECT" => Some(TokenKind::Select),
            "CASE" => Some(TokenKind::Case),
            "GOTO" => Some(TokenKind::GoTo),
            "GOSUB" => Some(TokenKind::GoSub),
            "ON" => Some(TokenKind::On),
            "PRINT" => Some(TokenKind::Print),
            "IMAGE" => Some(TokenKind::Image),
            "INPUT" => Some(TokenKind::Input_),
            "LINPUT" => Some(TokenKind::Linput),
            "ASSIGN" => Some(TokenKind::Assign),
            "OUTPUT" => Some(TokenKind::Output_),
            "ENTER" => Some(TokenKind::Enter),
            "READ" => Some(TokenKind::Read),
            "DATA" => Some(TokenKind::Data),
            "RESTORE" => Some(TokenKind::Restore),
            "DISP" => Some(TokenKind::Disp),
            "MAT" => Some(TokenKind::Mat),
            "LET" => Some(TokenKind::Let),
            "END" => Some(TokenKind::End),
            "STOP" => Some(TokenKind::Stop_),
            "PAUSE" => Some(TokenKind::Pause),
            "REM" => Some(TokenKind::Rem),
            "RANDOMIZE" => Some(TokenKind::Randomize),
            "WAIT" => Some(TokenKind::Wait_),
            "BEEP" => Some(TokenKind::Beep),
            "CONFIGURE" => Some(TokenKind::Configure),
            "CHANGE" => Some(TokenKind::Change),
            "AND" => Some(TokenKind::And),
            "OR" => Some(TokenKind::Or),
            "NOT" => Some(TokenKind::Not),
            "EXOR" => Some(TokenKind::Exor),
            "MOD" => Some(TokenKind::Mod_),
            "MODULO" => Some(TokenKind::Modulo),
            "DIV" => Some(TokenKind::Div_),
            "EXIT" => None,   // handled as EXIT IF
            "OPTION" => None, // handled as OPTION BASE
            "DEF" => None,    // handled as DEF FN
            _ => None,
        }
    }

    /// Check if a word is the start of a multi-word keyword sequence.
    /// Returns the number of additional words needed, if any.
    pub fn is_multiword_start(word: &str) -> Option<usize> {
        match word.to_uppercase().as_str() {
            "END" => Some(1),    // END IF, END WHILE, END LOOP, END SELECT
            "EXIT" => Some(1),   // EXIT IF
            "OPTION" => Some(1), // OPTION BASE
            "DEF" => Some(1),    // DEF FN
            "CASE" => Some(1),   // CASE ELSE
            "ON" => Some(1),     // ON ERROR
            "PRINT" => Some(1),  // PRINT USING
            _ => None,
        }
    }

    /// Given two consecutive words, try to form a compound keyword.
    pub fn match_compound(first: &str, second: &str) -> Option<TokenKind> {
        let combined = format!("{} {}", first.to_uppercase(), second.to_uppercase());
        match combined.as_str() {
            "END IF" => Some(TokenKind::EndIf),
            "END WHILE" => Some(TokenKind::EndWhile),
            "END LOOP" => Some(TokenKind::EndLoop),
            "END SELECT" => Some(TokenKind::EndSelect),
            "EXIT IF" => Some(TokenKind::ExitIf),
            "OPTION BASE" => Some(TokenKind::OptionBase),
            "DEF FN" => Some(TokenKind::DefFn),
            "CASE ELSE" => Some(TokenKind::CaseElse),
            "ON ERROR" => Some(TokenKind::OnError),
            "PRINT USING" => Some(TokenKind::PrintUsing),
            _ => None,
        }
    }
}

/// A single logical line of source, with its starting byte offset.
struct LogicalLine {
    text: String,
    offset: usize,
}

/// Split raw source into logical lines, handling `&` continuation.
fn split_logical_lines(source: &str) -> Vec<LogicalLine> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_offset = 0;
    let mut _line_start = 0;

    for (offset, ch) in source.char_indices() {
        if ch == '\n' || ch == '\r' {
            if ch == '\r' {
                continue; // skip \r, handle at \n
            }
            // Check if previous line ended with &
            let trimmed = current.trim_end();
            if trimmed.ends_with('&') {
                // Continuation line — keep accumulating without the &
                current = trimmed[..trimmed.len() - 1].to_string();
                current.push('\n');
            } else {
                if !current.trim().is_empty() {
                    lines.push(LogicalLine {
                        text: current.clone(),
                        offset: current_offset,
                    });
                }
                current.clear();
                current_offset = offset + 1;
            }
            _line_start = offset + 1;
        } else if ch == '\r' {
            // skip standalone \r
            if source.as_bytes().get(offset + 1) != Some(&b'\n') {
                let trimmed = current.trim_end();
                if trimmed.ends_with('&') {
                    current = trimmed[..trimmed.len() - 1].to_string();
                    current.push('\n');
                } else {
                    if !current.trim().is_empty() {
                        lines.push(LogicalLine {
                            text: current.clone(),
                            offset: current_offset,
                        });
                    }
                    current.clear();
                    current_offset = offset + 1;
                }
            }
        } else {
            current.push(ch);
        }
    }

    // Don't forget the last line
    if !current.trim().is_empty() {
        lines.push(LogicalLine {
            text: current,
            offset: current_offset,
        });
    }

    lines
}

/// The lexer state. Tokenizes HTBasic source one line at a time.
pub struct Lexer {
    #[allow(dead_code)]
    source: String,
    lines: Vec<LogicalLine>,
    line_index: usize,
    /// Tokens for the current line, yet to be yielded
    current_tokens: Vec<Token>,
    token_index: usize,
    /// Byte offset within the source
    pos: usize,
}

impl Lexer {
    pub fn new(source: String) -> Self {
        let lines = split_logical_lines(&source);
        let mut lexer = Self {
            source,
            lines,
            line_index: 0,
            current_tokens: Vec::new(),
            token_index: 0,
            pos: 0,
        };
        // Tokenize first line immediately
        if !lexer.lines.is_empty() {
            lexer.current_tokens = lexer.tokenize_line(0);
        } else {
            lexer.current_tokens = vec![Token::new(TokenKind::Eof, 0, 0)];
        }
        lexer
    }

    /// Return the next token without consuming it.
    pub fn peek(&self) -> &Token {
        if self.token_index < self.current_tokens.len() {
            &self.current_tokens[self.token_index]
        } else {
            // Return the last token (Eof)
            self.current_tokens.last().unwrap()
        }
    }

    /// Return the token kind of the next token without consuming it.
    pub fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    /// Consume and return the next token.
    pub fn advance(&mut self) -> Token {
        if self.token_index < self.current_tokens.len() {
            let token = self.current_tokens[self.token_index].clone();
            self.token_index += 1;

            // Move to next line if we've exhausted this one
            if self.token_index >= self.current_tokens.len()
                && self.line_index + 1 < self.lines.len()
            {
                self.line_index += 1;
                self.current_tokens = self.tokenize_line(self.line_index);
                self.token_index = 0;
            }

            token
        } else {
            // Return Eof
            Token::new(TokenKind::Eof, self.pos, self.pos)
        }
    }

    /// Tokenize a single logical line.
    fn tokenize_line(&mut self, line_idx: usize) -> Vec<Token> {
        let line = &self.lines[line_idx];
        let base_offset = line.offset;
        let text = line.text.as_str();

        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let char_indices: Vec<(usize, char)> = text.char_indices().collect();
        let mut i = 0;
        let len = chars.len();

        // Set pos to start of this line
        self.pos = base_offset;

        // --- Strip optional leading line number ---
        if i < len && chars[i].is_ascii_digit() {
            let start = i;
            while i < len && chars[i].is_ascii_digit() {
                i += 1;
            }
            // After digits, expect whitespace (not a colon — that would be a label starting with a digit)
            if i < len && (chars[i] == ' ' || chars[i] == '\t') {
                let num_str: String = chars[start..i].iter().collect();
                if let Ok(n) = num_str.parse::<i64>() {
                    tokens.push(Token::new(
                        TokenKind::IntegerLiteral(n),
                        base_offset + char_indices[start].0,
                        base_offset + char_indices[i - 1].0 + 1,
                    ));
                }
                // Skip whitespace after line number
                while i < len && (chars[i] == ' ' || chars[i] == '\t') {
                    i += 1;
                }
            } else {
                // It was something else starting with digits — reset
                i = start;
            }
        }

        // --- Tokenize the rest of the line ---
        while i < len {
            let ch = chars[i];
            let byte_pos = base_offset + char_indices[i].0;

            // Whitespace
            if ch == ' ' || ch == '\t' {
                i += 1;
                continue;
            }

            // Comment: ! (rest of line is comment)
            if ch == '!' {
                // HTBasic supports inline ! comments
                // Record the comment token and skip rest of line
                tokens.push(Token::new(
                    TokenKind::Bang,
                    byte_pos,
                    base_offset + text.len(),
                ));
                tokens.push(Token::new(
                    TokenKind::Newline,
                    base_offset + text.len(),
                    base_offset + text.len(),
                ));
                return tokens;
            }

            // Newline (end of logical line)
            if ch == '\n' {
                i += 1;
                continue;
            }

            // --- String literal ---
            if ch == '"' {
                let start = byte_pos;
                let mut s = String::new();
                i += 1; // skip opening "
                while i < len {
                    if chars[i] == '"' {
                        if i + 1 < len && chars[i + 1] == '"' {
                            // Escaped quote
                            s.push('"');
                            i += 2;
                        } else {
                            // End of string
                            i += 1;
                            break;
                        }
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                let end = base_offset + char_indices[i.min(len - 1)].0 + 1;
                tokens.push(Token::new(TokenKind::StringLiteral(s), start, end));
                continue;
            }

            // --- I/O path: @name ---
            if ch == '@' {
                i += 1;
                let start = byte_pos;
                let id_start = i;
                if i < len && (chars[i].is_ascii_alphabetic() || chars[i] == '_') {
                    while i < len
                        && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '$')
                    {
                        i += 1;
                    }
                    let id: String = chars[id_start..i].iter().collect();
                    let end = base_offset + char_indices[i - 1].0 + 1;
                    tokens.push(Token::new(TokenKind::IoPath(id), start, end));
                } else if i < len && chars[i].is_ascii_digit() {
                    // @ followed by digits (could be a numeric address)
                    while i < len && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    let id: String = chars[id_start..i].iter().collect();
                    let end = base_offset + char_indices[i - 1].0 + 1;
                    tokens.push(Token::new(TokenKind::IoPath(id), start, end));
                }
                continue;
            }

            // --- Numeric literal ---
            if ch.is_ascii_digit() || (ch == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
                let start = byte_pos;
                let num_str = self.lex_number(&chars, &mut i, &char_indices, base_offset);
                let end = if i < len {
                    base_offset + char_indices[i - 1].0 + 1
                } else {
                    base_offset + text.len()
                };

                // Try integer first, then float
                if let Ok(n) = num_str.parse::<i64>() {
                    tokens.push(Token::new(TokenKind::IntegerLiteral(n), start, end));
                } else if let Ok(n) = num_str.parse::<f64>() {
                    tokens.push(Token::new(TokenKind::RealLiteral(n), start, end));
                }
                continue;
            }

            // --- Hex literal &H... ---
            if ch == '&' {
                let start = byte_pos;
                if i + 1 < len {
                    let next = chars[i + 1].to_ascii_uppercase();
                    if next == 'H' {
                        i += 2;
                        let hex_start = i;
                        while i < len && chars[i].is_ascii_hexdigit() {
                            i += 1;
                        }
                        let hex_str: String = chars[hex_start..i].iter().collect();
                        if let Ok(n) = i64::from_str_radix(&hex_str, 16) {
                            let end = if i > 0 {
                                base_offset + char_indices[i - 1].0 + 1
                            } else {
                                start + 2
                            };
                            tokens.push(Token::new(TokenKind::IntegerLiteral(n), start, end));
                            continue;
                        }
                    } else if next == 'O' {
                        i += 2;
                        let oct_start = i;
                        while i < len && chars[i].is_ascii_digit() && chars[i] < '8' {
                            i += 1;
                        }
                        let oct_str: String = chars[oct_start..i].iter().collect();
                        if let Ok(n) = i64::from_str_radix(&oct_str, 8) {
                            let end = if i > 0 {
                                base_offset + char_indices[i - 1].0 + 1
                            } else {
                                start + 2
                            };
                            tokens.push(Token::new(TokenKind::IntegerLiteral(n), start, end));
                            continue;
                        }
                    }
                }
                // Not hex/octal — it's the & operator (string concatenation)
                tokens.push(Token::new(TokenKind::Amp, start, start + 1));
                i += 1;
                continue;
            }

            // --- Identifier, keyword, or label ---
            if ch.is_ascii_alphabetic() || ch == '_' {
                let start = byte_pos;
                let id_start = i;
                while i < len
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '$')
                {
                    i += 1;
                }
                let id_end = i;
                let word: String = chars[id_start..id_end].iter().collect();
                let word_end = base_offset + char_indices[id_end - 1].0 + 1;

                // Check if it's a label (identifier followed by colon at start of statement)
                // A label is at the beginning of a line (or right after a line number)
                let is_at_line_start = tokens.is_empty()
                    || (tokens.len() == 1
                        && matches!(tokens[0].kind, TokenKind::IntegerLiteral(_)));

                if is_at_line_start && i < len && chars[i] == ':' {
                    // Label definition
                    tokens.push(Token::new(TokenKind::LabelDef(word), start, word_end + 1));
                    i += 1; // skip colon
                    continue;
                }

                // Check if it's a multi-word keyword start (e.g., "END" → "END IF")
                if let Some(extra_words) = keywords::is_multiword_start(&word) {
                    let mut combined_words: Vec<(String, usize)> = vec![(word.clone(), id_start)];
                    let mut peek_i = i;

                    for _ in 0..extra_words {
                        while peek_i < len && (chars[peek_i] == ' ' || chars[peek_i] == '\t') {
                            peek_i += 1;
                        }
                        if peek_i < len
                            && (chars[peek_i].is_ascii_alphabetic() || chars[peek_i] == '_')
                        {
                            let w_start = peek_i;
                            while peek_i < len
                                && (chars[peek_i].is_ascii_alphanumeric() || chars[peek_i] == '_')
                            {
                                peek_i += 1;
                            }
                            let next_word: String = chars[w_start..peek_i].iter().collect();
                            combined_words.push((next_word, w_start));
                        } else {
                            break;
                        }
                    }

                    if combined_words.len() > 1 {
                        let first = &combined_words[0].0;
                        let mut matched = false;
                        for w in 1..combined_words.len() {
                            let (ref next_word, next_start) = combined_words[w];
                            if let Some(kw) = keywords::match_compound(first, next_word) {
                                let end_pos = base_offset + char_indices[peek_i.min(len - 1)].0 + 1;
                                tokens.push(Token::new(kw, start, end_pos));
                                i = peek_i;
                                matched = true;
                                break;
                            }
                            // Prefix match: e.g., DEF + FNSquare → "FN" matches prefix
                            let expected_prefix = match first.to_uppercase().as_str() {
                                "DEF" => "FN",
                                _ => "",
                            };
                            if !expected_prefix.is_empty()
                                && next_word.to_uppercase().starts_with(expected_prefix)
                                && next_word.len() > expected_prefix.len()
                            {
                                if let Some(kw) = keywords::match_compound(first, expected_prefix) {
                                    let prefix_end = next_start + expected_prefix.len();
                                    let end_pos =
                                        base_offset + char_indices[prefix_end.min(len - 1)].0;
                                    tokens.push(Token::new(kw, start, end_pos));
                                    i = prefix_end;
                                    matched = true;
                                    break;
                                }
                            }
                        }
                        if matched {
                            continue;
                        }
                    }
                }

                // Single-word keyword or identifier
                if let Some(kw) = keywords::match_keyword(&word) {
                    tokens.push(Token::new(kw, start, word_end));
                } else if word.ends_with('$') {
                    tokens.push(Token::new(
                        TokenKind::StringIdentifier(word),
                        start,
                        word_end,
                    ));
                } else {
                    tokens.push(Token::new(TokenKind::Identifier(word), start, word_end));
                }
                continue;
            }

            // --- Operators and punctuation ---
            match ch {
                '+' => {
                    tokens.push(Token::new(TokenKind::Plus, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '-' => {
                    tokens.push(Token::new(TokenKind::Minus, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '*' => {
                    tokens.push(Token::new(TokenKind::Star, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '/' => {
                    tokens.push(Token::new(TokenKind::Slash, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '^' => {
                    tokens.push(Token::new(TokenKind::Caret, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '\\' => {
                    tokens.push(Token::new(TokenKind::Backslash, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '.' => {
                    tokens.push(Token::new(TokenKind::Dot, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '(' => {
                    tokens.push(Token::new(TokenKind::LParen, byte_pos, byte_pos + 1));
                    i += 1;
                },
                ')' => {
                    tokens.push(Token::new(TokenKind::RParen, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '[' => {
                    tokens.push(Token::new(TokenKind::LBracket, byte_pos, byte_pos + 1));
                    i += 1;
                },
                ']' => {
                    tokens.push(Token::new(TokenKind::RBracket, byte_pos, byte_pos + 1));
                    i += 1;
                },
                ',' => {
                    tokens.push(Token::new(TokenKind::Comma, byte_pos, byte_pos + 1));
                    i += 1;
                },
                ';' => {
                    tokens.push(Token::new(TokenKind::Semicolon, byte_pos, byte_pos + 1));
                    i += 1;
                },
                ':' => {
                    tokens.push(Token::new(TokenKind::Colon, byte_pos, byte_pos + 1));
                    i += 1;
                },
                '<' => {
                    if i + 1 < len && chars[i + 1] == '>' {
                        tokens.push(Token::new(TokenKind::LtGt, byte_pos, byte_pos + 2));
                        i += 2;
                    } else if i + 1 < len && chars[i + 1] == '=' {
                        tokens.push(Token::new(TokenKind::LtEq, byte_pos, byte_pos + 2));
                        i += 2;
                    } else {
                        tokens.push(Token::new(TokenKind::Lt, byte_pos, byte_pos + 1));
                        i += 1;
                    }
                },
                '>' => {
                    if i + 1 < len && chars[i + 1] == '=' {
                        tokens.push(Token::new(TokenKind::GtEq, byte_pos, byte_pos + 2));
                        i += 2;
                    } else {
                        tokens.push(Token::new(TokenKind::Gt, byte_pos, byte_pos + 1));
                        i += 1;
                    }
                },
                '=' => {
                    tokens.push(Token::new(TokenKind::Eq, byte_pos, byte_pos + 1));
                    i += 1;
                },
                other => {
                    // Unknown character — skip with a generic token
                    tokens.push(Token::new(
                        TokenKind::Bang, // reuse as "unknown"
                        byte_pos,
                        byte_pos + other.len_utf8(),
                    ));
                    i += 1;
                },
            }
        }

        // Add newline at end of line
        let line_end = base_offset + text.len();
        tokens.push(Token::new(TokenKind::Newline, line_end, line_end));

        tokens
    }

    /// Lex a numeric literal, advancing `i` past it.
    fn lex_number(
        &self,
        chars: &[char],
        i: &mut usize,
        _char_indices: &[(usize, char)],
        _base_offset: usize,
    ) -> String {
        let start = *i;
        let len = chars.len();

        // Integer part
        while *i < len && chars[*i].is_ascii_digit() {
            *i += 1;
        }

        // Fractional part
        if *i < len && chars[*i] == '.' {
            *i += 1;
            while *i < len && chars[*i].is_ascii_digit() {
                *i += 1;
            }
        }

        // Exponent
        if *i < len && (chars[*i] == 'E' || chars[*i] == 'e') {
            *i += 1;
            if *i < len && (chars[*i] == '+' || chars[*i] == '-') {
                *i += 1;
            }
            while *i < len && chars[*i].is_ascii_digit() {
                *i += 1;
            }
        }

        // Trailing % (integer suffix) or # (double suffix) — skip them
        if *i < len && (chars[*i] == '%' || chars[*i] == '#') {
            *i += 1;
        }

        chars[start..*i].iter().collect()
    }
}

impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.advance();
        if token.kind == TokenKind::Eof {
            None
        } else {
            Some(token)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(source: &str) -> Vec<TokenKind> {
        let lexer = Lexer::new(source.to_string());
        lexer.map(|t| t.kind).collect()
    }

    #[test]
    fn test_simple_assignment() {
        let tokens = tokenize("LET X = 5\n");
        assert!(tokens.contains(&TokenKind::Let));
        assert!(tokens.contains(&TokenKind::Identifier("X".into())));
        assert!(tokens.contains(&TokenKind::IntegerLiteral(5)));
    }

    #[test]
    fn test_implicit_let() {
        let tokens = tokenize("A = 3.14\n");
        // "A" is an identifier, "=" is Eq, "3.14" is a real literal
        // With implicit LET, there is no LET keyword
        assert!(tokens.contains(&TokenKind::Identifier("A".into())));
        assert!(tokens.contains(&TokenKind::RealLiteral(3.14)));
    }

    #[test]
    fn test_if_then_else() {
        let tokens = tokenize("IF X > 0 THEN PRINT X ELSE PRINT -X\n");
        assert!(tokens.contains(&TokenKind::If));
        assert!(tokens.contains(&TokenKind::Then));
        assert!(tokens.contains(&TokenKind::Else));
    }

    #[test]
    fn test_end_if() {
        let tokens = tokenize("END IF\n");
        assert!(tokens.contains(&TokenKind::EndIf));
    }

    #[test]
    fn test_print_using() {
        let tokens = tokenize("PRINT USING \"###.##\"; A, B\n");
        assert!(tokens.contains(&TokenKind::PrintUsing));
    }

    #[test]
    fn test_string_variable() {
        let tokens = tokenize("Name$ = \"Hello\"\n");
        assert!(tokens.contains(&TokenKind::StringIdentifier("Name$".into())));
    }

    #[test]
    fn test_line_number() {
        let tokens = tokenize("100 PRINT X\n");
        assert!(tokens.contains(&TokenKind::IntegerLiteral(100)));
        assert!(tokens.contains(&TokenKind::Print));
    }

    #[test]
    fn test_label() {
        let tokens = tokenize("Start: PRINT \"Begin\"\n");
        assert!(tokens.contains(&TokenKind::LabelDef("Start".into())));
        assert!(tokens.contains(&TokenKind::Print));
    }

    #[test]
    fn test_comment_bang() {
        let tokens = tokenize("! This is a comment\nX = 1\n");
        assert!(tokens.contains(&TokenKind::Bang));
        // X=1 should still appear on next line
        assert!(tokens.contains(&TokenKind::Identifier("X".into())));
    }

    #[test]
    fn test_multi_statement() {
        let tokens = tokenize("X = 1 : Y = 2\n");
        assert!(tokens.contains(&TokenKind::Colon));
        assert!(tokens.contains(&TokenKind::Identifier("X".into())));
        assert!(tokens.contains(&TokenKind::Identifier("Y".into())));
    }

    #[test]
    fn test_for_loop() {
        let tokens = tokenize("FOR I = 1 TO 10 STEP 2\n");
        assert!(tokens.contains(&TokenKind::For));
        assert!(tokens.contains(&TokenKind::To));
        assert!(tokens.contains(&TokenKind::Step));
    }

    #[test]
    fn test_io_path() {
        let tokens = tokenize("ASSIGN @Meter TO 723\n");
        assert!(tokens.contains(&TokenKind::Assign));
        assert!(tokens.contains(&TokenKind::IoPath("Meter".into())));
    }
}
