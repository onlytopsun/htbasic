use crate::error::{HtBasicError, Result, Span};
use crate::lexer::lexer::Lexer;
use crate::lexer::token::{Token, TokenKind};
use crate::parser::ast::*;
use crate::parser::precedence::{binary_precedence, Precedence};

/// Graphics keywords recognized at statement start.
fn is_graphics_keyword(word: &str) -> bool {
    matches!(
        word,
        "GINIT"
            | "GCLEAR"
            | "PLOTTER"
            | "MOVE"
            | "DRAW"
            | "IMOVE"
            | "IDRAW"
            | "PLOT"
            | "PEN"
            | "LABEL"
            | "CSIZE"
            | "LDIR"
            | "LORG"
            | "GFONT"
            | "AXES"
            | "GRID"
            | "FRAME"
            | "CLIP"
            | "CLIP OFF"
            | "WINDOW"
            | "VIEWPORT"
            | "RECTANGLE"
            | "POLYGON"
            | "POLYLINE"
            | "GLOAD"
            | "GSTORE"
            | "PENUP"
            | "COLOR"
            | "SEPARATE"
            | "MERGE"
            | "LINE"
            | "AREA"
            | "INTENSITY"
            | "SET"
            | "DIGITIZE"
    )
}

/// Recursive descent parser with Pratt expression parsing.
pub struct Parser {
    lexer: Lexer,
    /// Lookahead token
    current: Token,
    /// Full source text, for raw-text capture (IMAGE formats).
    source: String,
}

impl Parser {
    pub fn new(source: String) -> Self {
        let src = source.clone();
        let mut lexer = Lexer::new(source);
        let current = lexer.advance();
        Self {
            lexer,
            current,
            source: src,
        }
    }

    // ===================== Helpers =====================

    fn advance(&mut self) {
        self.current = self.lexer.advance();
    }

    fn skip_newlines(&mut self) {
        while matches!(self.current.kind, TokenKind::Newline) {
            self.advance();
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token> {
        // Simple equality check — for token kinds with data, we only check the variant
        if std::mem::discriminant(&self.current.kind) == std::mem::discriminant(&kind) {
            let token = self.current.clone();
            self.advance();
            Ok(token)
        } else {
            Err(HtBasicError::ParseError {
                expected: kind.name().to_string(),
                found: self.current.kind.name().to_string(),
                span: self.current.span,
            })
        }
    }

    fn match_token(&mut self, kind: &TokenKind) -> bool {
        if std::mem::discriminant(&self.current.kind) == std::mem::discriminant(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn span(&self) -> Span {
        self.current.span
    }

    // ===================== Program =====================

    pub fn parse_program(&mut self) -> Result<Program> {
        let mut statements = Vec::new();
        let mut subprograms = Vec::new();
        let mut functions = Vec::new();

        self.skip_newlines();

        // Parse main program statements until END or EOF
        while !matches!(self.current.kind, TokenKind::Eof | TokenKind::End) {
            // Subprogram / function sections — possibly behind a line number
            // (`10 SUB Foo(...)`). If we see SUB/DEF FN before END, treat
            // remaining statements as the main body and then parse SUBs.
            let section_start = matches!(self.current.kind, TokenKind::Sub | TokenKind::DefFn)
                || (matches!(self.current.kind, TokenKind::IntegerLiteral(_))
                    && matches!(self.lexer.peek().kind, TokenKind::Sub | TokenKind::DefFn));
            if section_start {
                if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
                    if let TokenKind::IntegerLiteral(ref n) = self.current.kind {
                        let num = n.to_string();
                        statements.push(Stmt::Rem(format!("__label__{}", num), Span::new(0, 0)));
                    }
                    self.advance();
                    self.skip_newlines();
                }
                break;
            }

            let stmts = self.parse_statement_or_line()?;
            statements.extend(stmts);
            self.skip_newlines();
        }

        // Consume END if present
        if matches!(self.current.kind, TokenKind::End) {
            statements.push(Stmt::End(self.span()));
            self.advance();
            self.skip_newlines();
        }

        // Parse SUB and DEF FN definitions after END
        loop {
            self.skip_newlines();
            match &self.current.kind {
                TokenKind::Sub => {
                    let sub = self.parse_subprogram()?;
                    subprograms.push(sub);
                },
                TokenKind::DefFn => {
                    let func = self.parse_fn_def()?;
                    functions.push(func);
                },
                TokenKind::Eof => break,
                _ => {
                    // SUB/DEF FN behind a line number (`140 SUB Prtmat(...)`).
                    if matches!(self.current.kind, TokenKind::IntegerLiteral(_))
                        && matches!(self.lexer.peek().kind, TokenKind::Sub | TokenKind::DefFn)
                    {
                        if let TokenKind::IntegerLiteral(ref n) = self.current.kind {
                            let num = n.to_string();
                            statements.push(Stmt::Rem(format!("__label__{}", num), Span::new(0, 0)));
                        }
                        self.advance();
                        continue;
                    }
                    // Labels and statements after END are valid for GOSUB/GOTO targets.
                    // Parse them and add to main program statements.
                    if let Ok(stmts) = self.parse_statement_or_line() {
                        statements.extend(stmts);
                    } else {
                        self.advance();
                    }
                },
            }
        }

        Ok(Program {
            statements,
            subprograms,
            functions,
        })
    }

    /// Parse a single statement or multi-statement line (colon-separated).
    fn parse_statement_or_line(&mut self) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();

        // Line number at the start of a line: register it as a jump target
        // (`GOTO 380`) and skip. Converted TransEra programs use line
        // numbers for every line.
        if let TokenKind::IntegerLiteral(ref n) = self.current.kind {
            let num = n.to_string();
            self.advance();
            stmts.push(Stmt::Rem(format!("__label__{}", num), Span::new(0, 0)));
            if matches!(self.current.kind, TokenKind::Newline | TokenKind::Eof) {
                return Ok(stmts);
            }
        }

        // Check for label at start of line
        if let TokenKind::LabelDef(ref label) = self.current.kind {
            let label_name = label.clone();
            self.advance();
            stmts.push(Stmt::Rem(
                format!("__label__{}", label_name),
                Span::new(0, 0),
            ));
            // After a label, parse the rest of the line
            if !matches!(self.current.kind, TokenKind::Newline | TokenKind::Eof) {
                let rest = self.parse_statement_or_line()?;
                stmts.extend(rest);
            }
            return Ok(stmts);
        }

        stmts.push(self.parse_statement()?);

        // Handle colon- and semicolon-separated multi-statement lines
        // (HP BASIC accepts `;` as a statement separator, e.g.
        // `PLOTTER IS CRT,"INTERNAL"; COLOR MAP`).
        while matches!(self.current.kind, TokenKind::Colon | TokenKind::Semicolon) {
            self.advance(); // consume separator
            self.skip_newlines();
            if !matches!(self.current.kind, TokenKind::Newline | TokenKind::Eof) {
                stmts.push(self.parse_statement()?);
            }
        }

        Ok(stmts)
    }

    /// Converted TransEra programs put a line number on every line,
    /// including block terminators (`260 NEXT Row`, `340 END WHILE`).
    /// If the current token is a line number immediately followed by one
    /// of `kinds`, consume the line number and return a label statement
    /// for it; the caller inserts the label and then sees the keyword
    /// as the current token. Returns None otherwise (position unchanged).
    fn consume_labeled_kw(&mut self, kinds: &[TokenKind]) -> Option<Stmt> {
        if let TokenKind::IntegerLiteral(ref n) = self.current.kind {
            if kinds.iter().any(|k| {
                std::mem::discriminant(&self.lexer.peek().kind) == std::mem::discriminant(k)
            }) {
                let num = n.to_string();
                self.advance();
                return Some(Stmt::Rem(format!("__label__{}", num), Span::new(0, 0)));
            }
        }
        None
    }

    // ===================== Statements =====================

    fn parse_statement(&mut self) -> Result<Stmt> {
        self.skip_newlines();

        match &self.current.kind {
            // Comments
            TokenKind::Rem => {
                let span = self.span();
                self.advance();
                // REM consumes the rest of the line (a colon starts a new
                // statement on the same line).
                while !matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    self.advance();
                }
                Ok(Stmt::Rem(String::new(), span))
            },
            TokenKind::Bang => {
                let span = self.span();
                self.advance();
                Ok(Stmt::Comment(String::new(), span))
            },

            // Declarations
            TokenKind::Dim => self.parse_dim(),
            TokenKind::Com => self.parse_com(),
            TokenKind::Real
            | TokenKind::Integer
            | TokenKind::Short
            | TokenKind::Long
            | TokenKind::Complex => self.parse_type_decl(),
            TokenKind::OptionBase => self.parse_option_base(),

            // Control flow
            TokenKind::If => self.parse_if(),
            TokenKind::For => self.parse_for(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop_ => self.parse_loop(),
            TokenKind::Repeat => self.parse_repeat(),
            TokenKind::Select => self.parse_select(),
            TokenKind::ExitIf => {
                let span = self.span();
                self.advance();
                let cond = self.parse_expression()?;
                Ok(Stmt::ExitIf(Box::new(cond), span))
            },
            TokenKind::GoTo => self.parse_goto(),
            TokenKind::GoSub => self.parse_gosub(),
            TokenKind::Next => {
                // Standalone NEXT (array-iteration form when the FOR body
                // does not capture it) — consume and ignore.
                let span = self.span();
                self.advance();
                if matches!(self.current.kind, TokenKind::Identifier(_)) {
                    self.advance();
                    if matches!(self.current.kind, TokenKind::LParen) {
                        self.advance();
                        if matches!(self.current.kind, TokenKind::Star) {
                            self.advance();
                        }
                        self.expect(TokenKind::RParen)?;
                    }
                }
                Ok(Stmt::Rem("NEXT".into(), span))
            },
            TokenKind::On | TokenKind::OnError => self.parse_on(),
            TokenKind::Return => {
                let span = self.span();
                self.advance();
                // Optional expression (for DEF FN)
                let expr = if !matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Ok(Stmt::Return(expr, span))
            },

            // I/O
            TokenKind::Print => self.parse_print(),
            TokenKind::PrintUsing => self.parse_print_using(),
            TokenKind::Image => {
                let span = self.span();
                self.advance();
                // IMAGE formats are free-form text (e.g.
                // `IMAGE 3("[",3DD.DD,"]",/)`) that does not tokenize as
                // an expression; capture the raw source text to end of
                // line instead.
                let start = if matches!(self.current.kind, TokenKind::Newline | TokenKind::Eof)
                {
                    span.end
                } else {
                    self.current.span.start
                };
                let mut end = start;
                while !matches!(self.current.kind, TokenKind::Newline | TokenKind::Eof) {
                    end = self.current.span.end;
                    self.advance();
                }
                let format = self.source[start.min(self.source.len())..end.min(self.source.len())]
                    .to_string();
                Ok(Stmt::Image(format, span))
            },
            TokenKind::Input_ => self.parse_input(),
            TokenKind::Linput => self.parse_linput(),
            TokenKind::Read => self.parse_read(),
            TokenKind::Data => self.parse_data(),
            TokenKind::Restore => self.parse_restore(),
            TokenKind::Disp => {
                let span = self.span();
                self.advance();
                // DISP [expr|"text"] — the argument is optional (bare
                // `DISP` cancels the message line) and may be an
                // expression, not just a string literal.
                let msg = if matches!(
                    self.current.kind,
                    TokenKind::Newline
                        | TokenKind::Eof
                        | TokenKind::Colon
                        | TokenKind::Semicolon
                ) {
                    String::new()
                } else if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                    self.expect_string()?
                } else {
                    let start = self.current.span.start;
                    let _ = self.parse_expression()?;
                    let end = self.current.span.start;
                    self.source[start.min(self.source.len())..end.min(self.source.len())]
                        .trim()
                        .to_string()
                };
                Ok(Stmt::Disp(msg, span))
            },
            TokenKind::Assign => self.parse_assign(),
            TokenKind::Output_ => self.parse_output(),
            TokenKind::Enter => self.parse_enter_stmt(),

            // Subprogram calls
            TokenKind::Call => self.parse_call(),
            TokenKind::SubExit => {
                let span = self.span();
                self.advance();
                Ok(Stmt::Return(None, span))
            },

            // Matrix
            TokenKind::Mat => self.parse_mat(),

            // Other
            TokenKind::Let => {
                self.advance();
                self.parse_implicit_let()
            },
            TokenKind::Stop_ => {
                let span = self.span();
                self.advance();
                Ok(Stmt::Stop(span))
            },
            TokenKind::Pause => {
                let span = self.span();
                self.advance();
                Ok(Stmt::Pause(span))
            },
            TokenKind::End => {
                let span = self.span();
                self.advance();
                Ok(Stmt::End(span))
            },
            TokenKind::Beep => {
                let span = self.span();
                self.advance();
                Ok(Stmt::Beep(span))
            },
            TokenKind::Wait_ => {
                let span = self.span();
                self.advance();
                let expr = self.parse_expression()?;
                Ok(Stmt::Wait(expr, span))
            },
            TokenKind::Randomize => {
                let span = self.span();
                self.advance();
                let seed = if !matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Ok(Stmt::Randomize(seed, span))
            },
            TokenKind::Configure => {
                let span = self.span();
                self.advance();
                // Parse multi-word key: CONFIGURE LABEL "text", CONFIGURE DUMP TO "PCL", etc.
                let mut key_parts = vec![self.expect_identifier()?];
                // Consume additional key words until we hit a non-identifier or end of line
                while matches!(self.current.kind, TokenKind::Identifier(_)) {
                    let next = match &self.current.kind {
                        TokenKind::Identifier(s) => s.to_uppercase(),
                        _ => break,
                    };
                    // Stop at keywords that indicate a new statement
                    let stop_words = [
                        "PRINT", "IF", "FOR", "WHILE", "GOTO", "GOSUB", "CALL", "END", "DIM",
                    ];
                    if stop_words.contains(&next.as_str()) {
                        break;
                    }
                    key_parts.push(self.expect_identifier()?);
                }
                let key = key_parts.join(" ");
                // Parse value: could be identifier, integer, or string
                let value = match &self.current.kind {
                    TokenKind::StringLiteral(s) => {
                        let v = s.clone();
                        self.advance();
                        // Consume more value parts if present
                        let mut parts = vec![v];
                        while !matches!(
                            self.current.kind,
                            TokenKind::Newline
                                | TokenKind::Eof
                                | TokenKind::Colon
                                | TokenKind::Semicolon
                        ) {
                            if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                                parts.push(self.expect_string()?);
                            } else if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
                                parts.push(self.expect_integer()?.to_string());
                            } else if matches!(self.current.kind, TokenKind::Identifier(_)) {
                                parts.push(self.expect_identifier()?);
                            } else {
                                break;
                            }
                        }
                        parts.join(" ")
                    },
                    TokenKind::IntegerLiteral(_) => self.expect_integer()?.to_string(),
                    TokenKind::Identifier(_) => self.expect_identifier()?,
                    // `CONFIGURE MSI ON` / `CONFIGURE SAVE ASCII ON` — ON is
                    // a keyword token, not an identifier.
                    TokenKind::On => {
                        self.advance();
                        "ON".to_string()
                    },
                    // `CONFIGURE DUMP TO "PCL"` — keyword plus value parts.
                    TokenKind::To => {
                        self.advance();
                        let mut parts = vec!["TO".to_string()];
                        while matches!(
                            self.current.kind,
                            TokenKind::StringLiteral(_) | TokenKind::Identifier(_)
                        ) {
                            if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                                parts.push(self.expect_string()?);
                            } else {
                                parts.push(self.expect_identifier()?);
                            }
                        }
                        parts.join(" ")
                    },
                    _ => String::new(),
                };
                Ok(Stmt::Configure(key, value, span))
            },

            // Event handling: ENABLE, DISABLE, OFF
            TokenKind::Identifier(name)
                if { matches!(name.to_uppercase().as_str(), "ENABLE" | "DISABLE" | "OFF") } =>
            {
                let kw = name.clone().to_uppercase();
                self.advance();
                let span = self.span();
                if kw == "ENABLE" {
                    Ok(Stmt::Configure("ENABLE".into(), "".into(), span))
                } else if kw == "DISABLE" {
                    Ok(Stmt::Configure("DISABLE".into(), "".into(), span))
                } else {
                    // OFF KEY, OFF CYCLE, OFF END @File, etc. — event names
                    // can be word-like keyword tokens (END) as well as
                    // identifiers.
                    let event = match &self.current.kind {
                        TokenKind::Identifier(s) | TokenKind::StringIdentifier(s) => {
                            let e = s.clone();
                            self.advance();
                            e
                        },
                        TokenKind::End => {
                            self.advance();
                            "END".to_string()
                        },
                        _ => {
                            return Err(HtBasicError::ParseError {
                                expected: "identifier".into(),
                                found: self.current.kind.name().into(),
                                span: self.span(),
                            });
                        },
                    };
                    Ok(Stmt::Configure(
                        format!("OFF {}", event.to_uppercase()),
                        "".into(),
                        span,
                    ))
                }
            },

            // System keywords: STATUS, CONTROL, TRANSFER, CLEAR, TRIGGER, ABORT, etc.
            TokenKind::Identifier(name)
                if {
                    let u = name.to_uppercase();
                    matches!(
                        u.as_str(),
                        "STATUS"
                            | "CONTROL"
                            | "TRANSFER"
                            | "CLEAR"
                            | "TRIGGER"
                            | "ABORT"
                            | "CHAIN"
                            | "SCRATCH"
                            | "CAT"
                            | "CREATE"
                            | "PURGE"
                            | "RENAME"
                            | "COPY"
                            | "MASS"
                            | "XREF"
                            | "PROG"
                            | "BLOAD"
                            | "BSTORE"
                            | "DUMP"
                            | "READIO"
                            | "WRITEIO"
                            | "RESET"
                            | "GRAPHICS"
                            | "ALPHA"
                            | "KBD"
                            | "TRACK"
                            | "DISPLAY"
                            | "SYMBOL"
                            | "KEY"
                            | "TRACE"
                    )
                } =>
            {
                let kw = name.clone().to_uppercase();
                let span = self.span();
                self.advance();
                // Consume rest of line as value
                let mut parts = Vec::new();
                while !matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    match &self.current.kind {
                        TokenKind::StringLiteral(s) => {
                            parts.push(s.clone());
                            self.advance();
                        },
                        TokenKind::IntegerLiteral(n) => {
                            parts.push(n.to_string());
                            self.advance();
                        },
                        TokenKind::Identifier(s) => {
                            parts.push(s.clone());
                            self.advance();
                        },
                        TokenKind::IoPath(s) => {
                            parts.push(format!("@{}", s));
                            self.advance();
                        },
                        TokenKind::Comma | TokenKind::Semicolon => {
                            self.advance();
                        },
                        _ => {
                            self.advance();
                        },
                    }
                }
                Ok(Stmt::Configure(kw, parts.join(" "), span))
            },

            // Graphics keywords
            TokenKind::Identifier(name) => {
                let upper = name.to_uppercase();
                if is_graphics_keyword(&upper) {
                    return self.parse_graphics_cmd(&upper);
                }
                self.parse_implicit_or_expression_stmt()
            },
            // Implicit LET, assignment, or unrecognized
            _ => {
                // Try parsing as implicit LET or handle the token
                self.parse_implicit_or_expression_stmt()
            },
        }
    }

    // ===================== Declaration Parsing =====================

    fn parse_dim(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // consume DIM
        let mut entries = Vec::new();

        loop {
            let name = self.expect_identifier_or_string()?;
            let mut dimensions = Vec::new();

            if matches!(self.current.kind, TokenKind::LParen | TokenKind::LBracket) {
                self.advance(); // ( or [
                loop {
                    let lower = if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
                        let n = self.expect_integer()?;
                        if matches!(self.current.kind, TokenKind::Colon) {
                            self.advance(); // :
                            let upper = self.expect_integer()?;
                            (n, upper)
                        } else {
                            (0, n) // upper bound only
                        }
                    } else {
                        let upper = self.expect_integer()?;
                        (0, upper)
                    };
                    dimensions.push(lower);

                    if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                // 03 00 dialect binaries pair `[` with `)`
                // (`DIM Matrix[3,3)` — csum.prg).
                if !matches!(self.current.kind, TokenKind::RParen | TokenKind::RBracket) {
                    return Err(HtBasicError::ParseError {
                        expected: ") or ]".into(),
                        found: self.current.kind.name().into(),
                        span: self.span(),
                    });
                }
                self.advance();
            }

            entries.push(DimEntry { name, dimensions });

            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(Stmt::Dim(entries, span))
    }

    fn parse_com(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // consume COM

        // Optional /BlockName/
        let block_name = if matches!(self.current.kind, TokenKind::Slash) {
            self.advance();
            let name = self.expect_identifier()?;
            self.expect(TokenKind::Slash)?;
            Some(name)
        } else {
            None
        };

        let mut entries = Vec::new();

        loop {
            // Type specifier: /REAL/, /INTEGER/, etc.
            let var_type = if matches!(self.current.kind, TokenKind::Slash) {
                self.advance();
                let type_kw = self.expect_identifier()?.to_uppercase();
                self.expect(TokenKind::Slash)?;
                match type_kw.as_str() {
                    "REAL" => VarType::Real,
                    "INTEGER" => VarType::Integer,
                    "SHORT" => VarType::Short,
                    "LONG" => VarType::Long,
                    "COMPLEX" => VarType::Complex,
                    "STRING" => VarType::String_,
                    _ => VarType::Real,
                }
            } else {
                VarType::Real
            };

            let name = self.expect_identifier()?;
            let mut dimensions = Vec::new();

            if matches!(self.current.kind, TokenKind::LParen) {
                self.advance();
                loop {
                    let upper = self.expect_integer()?;
                    dimensions.push((0, upper));
                    if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RParen)?;
            }

            entries.push(ComEntry {
                var_type,
                name,
                dimensions,
            });

            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(Stmt::Com(
            ComBlock {
                name: block_name,
                entries,
            },
            span,
        ))
    }

    fn parse_type_decl(&mut self) -> Result<Stmt> {
        let var_type = match self.current.kind {
            TokenKind::Real => VarType::Real,
            TokenKind::Integer => VarType::Integer,
            TokenKind::Short => VarType::Short,
            TokenKind::Long => VarType::Long,
            TokenKind::Complex => VarType::Complex,
            _ => VarType::Real,
        };
        let span = self.span();
        self.advance();

        let mut entries = Vec::new();
        loop {
            let name = self.expect_identifier()?;
            let dims = Vec::new();
            entries.push(ComEntry {
                var_type: var_type.clone(),
                name,
                dimensions: dims,
            });

            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        // Type declarations are like COM without a block name
        Ok(Stmt::Com(
            ComBlock {
                name: None,
                entries,
            },
            span,
        ))
    }

    fn parse_option_base(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // OPTION BASE
        let base = if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
            self.expect_integer()?
        } else {
            0
        };
        Ok(Stmt::OptionBase(base, span))
    }

    // ===================== Control Flow Parsing =====================

    fn parse_if(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // IF

        let condition = self.parse_expression()?;

        // Expect THEN
        self.expect(TokenKind::Then)?;

        // Check if single-line IF: statement on same line, possibly with ELSE
        let then_stmt = self.parse_statement()?;

        if matches!(self.current.kind, TokenKind::Else) {
            self.advance(); // ELSE
            self.skip_newlines();
            let else_stmt = self.parse_statement()?;
            Ok(Stmt::SingleLineIf(
                condition,
                Box::new(then_stmt),
                Some(Box::new(else_stmt)),
                span,
            ))
        } else if matches!(self.current.kind, TokenKind::Newline | TokenKind::Colon) {
            // No ELSE on same line — treat as single-line IF
            Ok(Stmt::SingleLineIf(
                condition,
                Box::new(then_stmt),
                None,
                span,
            ))
        } else {
            // Multi-line IF block (THEN on same line, more statements follow on same line?)
            // Actually, for simplicity, treat everything after THEN as multi-line
            let mut then_body = vec![then_stmt];
            let mut else_ifs = Vec::new();
            let mut else_body = None;

            loop {
                self.skip_newlines();
                // `340 END IF` — line number precedes the terminator.
                if let Some(lbl) = self.consume_labeled_kw(&[
                    TokenKind::Else,
                    TokenKind::EndIf,
                    TokenKind::SubEnd,
                    TokenKind::FnEnd,
                    TokenKind::End,
                ]) {
                    then_body.push(lbl);
                }
                match &self.current.kind {
                    TokenKind::Else => {
                        self.advance();
                        self.skip_newlines();
                        // Check for ELSE IF
                        if matches!(self.current.kind, TokenKind::If) {
                            self.advance();
                            let cond = self.parse_expression()?;
                            self.expect(TokenKind::Then)?;
                            let body =
                                self.parse_block_until(&[TokenKind::Else, TokenKind::EndIf])?;
                            else_ifs.push((cond, body));
                        } else {
                            else_body = Some(self.parse_block_until(&[TokenKind::EndIf])?);
                            break;
                        }
                    },
                    TokenKind::EndIf => {
                        self.advance();
                        break;
                    },
                    TokenKind::SubEnd | TokenKind::FnEnd | TokenKind::End | TokenKind::Eof => {
                        break;
                    },
                    _ => {
                        // More statements in THEN body
                        if matches!(self.current.kind, TokenKind::Newline) {
                            self.advance();
                            continue;
                        }
                        let stmts = self.parse_statement_or_line()?;
                        then_body.extend(stmts);
                    },
                }
            }

            Ok(Stmt::If(
                IfBlock {
                    condition,
                    then_body,
                    else_ifs,
                    else_body,
                    span,
                },
                span,
            ))
        }
    }

    fn parse_for(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // FOR

        let var = self.expect_identifier()?;
        if matches!(self.current.kind, TokenKind::LParen) {
            // Array iteration: `FOR B(*)` ... `NEXT B(*)`. No counter or
            // limits — iterate over the array elements. Represent as a
            // 0..1 loop (parse-clean stand-in).
            self.advance();
            if matches!(self.current.kind, TokenKind::Star) {
                self.advance();
            }
            self.expect(TokenKind::RParen)?;
            self.skip_newlines();
            let mut body = Vec::new();
            loop {
                self.skip_newlines();
                // `260 NEXT B(*)` — line number precedes NEXT.
                if let Some(lbl) = self.consume_labeled_kw(&[TokenKind::Next]) {
                    body.push(lbl);
                }
                if matches!(self.current.kind, TokenKind::Next) {
                    self.advance();
                    // Optional array name and (*) after NEXT
                    if matches!(self.current.kind, TokenKind::Identifier(_)) {
                        self.advance();
                        if matches!(self.current.kind, TokenKind::LParen) {
                            self.advance();
                            if matches!(self.current.kind, TokenKind::Star) {
                                self.advance();
                            }
                            self.expect(TokenKind::RParen)?;
                        }
                    }
                    break;
                }
                if matches!(self.current.kind, TokenKind::Eof) {
                    break;
                }
                let stmts = self.parse_statement_or_line()?;
                body.extend(stmts);
            }
            return Ok(Stmt::For(
                var,
                Expr::Integer(0, span),
                Expr::Integer(1, span),
                None,
                body,
                span,
            ));
        }
        self.expect(TokenKind::Eq)?;
        let start = self.parse_expression()?;
        self.expect(TokenKind::To)?;
        let end = self.parse_expression()?;

        let step = if matches!(self.current.kind, TokenKind::Step) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        self.skip_newlines();

        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            // `260 NEXT Row` — line number precedes NEXT.
            if let Some(lbl) = self.consume_labeled_kw(&[TokenKind::Next]) {
                body.push(lbl);
            }
            if matches!(self.current.kind, TokenKind::Next) {
                self.advance();
                // Optionally consume variable name and whole-array marker
                if matches!(self.current.kind, TokenKind::Identifier(_)) {
                    self.advance();
                    if matches!(self.current.kind, TokenKind::LParen) {
                        self.advance();
                        if matches!(self.current.kind, TokenKind::Star) {
                            self.advance();
                        }
                        self.expect(TokenKind::RParen)?;
                    }
                }
                break;
            }
            if matches!(self.current.kind, TokenKind::Eof) {
                break;
            }
            let stmts = self.parse_statement_or_line()?;
            body.extend(stmts);
        }

        Ok(Stmt::For(var, start, end, step, body, span))
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // WHILE

        let condition = self.parse_expression()?;

        let body = self.parse_block_until(&[TokenKind::EndWhile])?;
        if matches!(self.current.kind, TokenKind::EndWhile) {
            self.advance();
        }

        Ok(Stmt::While(condition, body, span))
    }

    fn parse_loop(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // LOOP

        let body = self.parse_block_until(&[TokenKind::EndLoop])?;

        if matches!(self.current.kind, TokenKind::EndLoop) {
            self.advance();
        }

        Ok(Stmt::Loop_(body, span))
    }

    fn parse_repeat(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // REPEAT

        let body = self.parse_block_until(&[TokenKind::Until])?;

        self.expect(TokenKind::Until)?;
        let condition = self.parse_expression()?;

        Ok(Stmt::Repeat(body, condition, span))
    }

    fn parse_select(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // SELECT

        let expr = self.parse_expression()?;

        let mut arms: Vec<CaseArm> = Vec::new();

        loop {
            self.skip_newlines();
            // `120 CASE 1` — line number precedes the CASE keyword.
            if let Some(lbl) = self.consume_labeled_kw(&[
                TokenKind::Case,
                TokenKind::CaseElse,
                TokenKind::EndSelect,
            ]) {
                if let Some(last) = arms.last_mut() {
                    last.body.push(lbl);
                }
            }
            match &self.current.kind {
                TokenKind::Case => {
                    self.advance();
                    let mut cases = Vec::new();

                    loop {
                        if matches!(self.current.kind, TokenKind::CaseElse) {
                            self.advance();
                            cases.push(CaseValue::Else);
                            break;
                        }

                        // Check for CASE IS
                        if matches!(self.current.kind, TokenKind::Identifier(_)) {
                            // Could be "IS" keyword, but HTBasic typically uses relational ops directly
                            // Parse as expression, check if it looks like "IS op expr"
                        }

                        // Relational shorthand: `CASE < 1` means
                        // `CASE selectvar < 1` (HTBasic CASE EXAMPLE).
                        let val = if matches!(
                            self.current.kind,
                            TokenKind::Lt
                                | TokenKind::Gt
                                | TokenKind::LtEq
                                | TokenKind::GtEq
                                | TokenKind::LtGt
                        ) {
                            let op = match self.current.kind {
                                TokenKind::Lt => BinaryOp::Lt,
                                TokenKind::Gt => BinaryOp::Gt,
                                TokenKind::LtEq => BinaryOp::LtEq,
                                TokenKind::GtEq => BinaryOp::GtEq,
                                _ => BinaryOp::NotEq,
                            };
                            self.advance();
                            let rhs = self.parse_expression()?;
                            Expr::Binary(Box::new(expr.clone()), op, Box::new(rhs), span)
                        } else {
                            self.parse_expression()?
                        };

                        if matches!(self.current.kind, TokenKind::To) {
                            self.advance();
                            let high = self.parse_expression()?;
                            cases.push(CaseValue::Range(val, high));
                        } else {
                            cases.push(CaseValue::Single(val));
                        }

                        if matches!(self.current.kind, TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }

                    self.skip_newlines();
                    let mut body = Vec::new();
                    loop {
                        self.skip_newlines();
                        if let Some(lbl) = self.consume_labeled_kw(&[
                            TokenKind::Case,
                            TokenKind::CaseElse,
                            TokenKind::EndSelect,
                        ]) {
                            body.push(lbl);
                        }
                        match &self.current.kind {
                            TokenKind::Case | TokenKind::CaseElse | TokenKind::EndSelect => {
                                break;
                            },
                            TokenKind::Eof => break,
                            _ => {
                                let stmts = self.parse_statement_or_line()?;
                                body.extend(stmts);
                            },
                        }
                    }

                    arms.push(CaseArm { cases, body });
                },
                TokenKind::CaseElse => {
                    self.advance();
                    let mut body = Vec::new();
                    loop {
                        self.skip_newlines();
                        if let Some(lbl) = self.consume_labeled_kw(&[
                            TokenKind::Case,
                            TokenKind::EndSelect,
                        ]) {
                            body.push(lbl);
                        }
                        match &self.current.kind {
                            TokenKind::Case | TokenKind::EndSelect => break,
                            TokenKind::Eof => break,
                            _ => {
                                let stmts = self.parse_statement_or_line()?;
                                body.extend(stmts);
                            },
                        }
                    }
                    arms.push(CaseArm {
                        cases: vec![CaseValue::Else],
                        body,
                    });
                },
                TokenKind::EndSelect => {
                    self.advance();
                    break;
                },
                _ => break,
            }
        }

        Ok(Stmt::Select(expr, arms, span))
    }

    fn parse_goto(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // GOTO
        let label = self.expect_identifier_or_integer()?;
        Ok(Stmt::GoTo(label, span))
    }

    fn parse_gosub(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // GOSUB
        let label = self.expect_identifier_or_integer()?;
        Ok(Stmt::GoSub(label, span))
    }

    fn parse_on_computed(&mut self, span: Span) -> Result<Stmt> {
        let expr = self.parse_expression()?;
        match &self.current.kind {
            TokenKind::GoTo => {
                self.advance();
                let mut labels = Vec::new();
                loop {
                    labels.push(self.expect_identifier_or_integer()?);
                    if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                Ok(Stmt::OnGoTo(expr, labels, span))
            },
            TokenKind::GoSub => {
                self.advance();
                let mut labels = Vec::new();
                loop {
                    labels.push(self.expect_identifier_or_integer()?);
                    if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                Ok(Stmt::OnGoSub(expr, labels, span))
            },
            _ => Err(HtBasicError::ParseError {
                expected: "GOTO or GOSUB".into(),
                found: self.current.kind.name().into(),
                span: self.span(),
            }),
        }
    }

    /// Rest of `ON ERROR`: an optional GOTO with a line number, label,
    /// string identifier, or I/O path target (`ON ERROR GOTO @File` —
    /// assign.prg's name-table ref C7 00 resolves to @File).
    fn parse_on_error_rest(&mut self, span: Span) -> Result<Stmt> {
        if matches!(self.current.kind, TokenKind::GoTo) {
            self.advance();
            let label = match &self.current.kind {
                TokenKind::IoPath(name) => {
                    let n = name.clone();
                    self.advance();
                    n
                },
                TokenKind::StringIdentifier(name) => {
                    let n = name.clone();
                    self.advance();
                    n
                },
                _ => self.expect_identifier_or_integer()?,
            };
            Ok(Stmt::GoTo(format!("__onerror__{}", label), span))
        } else {
            Ok(Stmt::Rem("ON ERROR".into(), span))
        }
    }

    fn parse_on(&mut self) -> Result<Stmt> {
        let span = self.span();
        // The lexer may emit the `ON ERROR` compound as a single token, in
        // which case the ERROR keyword is already consumed.
        if matches!(self.current.kind, TokenKind::OnError) {
            self.advance();
            return self.parse_on_error_rest(span);
        }
        self.advance(); // ON

        // Could be ON ERROR GOTO, ON expr GOTO, ON expr GOSUB, ON KEY, ON CYCLE, etc.
        if matches!(self.current.kind, TokenKind::OnError) {
            self.advance();
            return self.parse_on_error_rest(span);
        } else {
            // Check for event keywords (may be Identifier or keyword tokens like END, HALT)
            let event_kw = match &self.current.kind {
                TokenKind::Identifier(ref id) => Some(id.to_uppercase()),
                TokenKind::End => Some("END".to_string()),
                TokenKind::Stop_ => Some("HALT".to_string()),
                _ => None,
            };
            let event_events = [
                "KEY", "CYCLE", "KBD", "KNOB", "END", "HALT", "TIMEOUT", "SIGNAL", "DELAY", "INTR",
                "TIME",
            ];
            if let Some(ref kw) = event_kw {
                if event_events.contains(&kw.as_str()) {
                    self.advance(); // consume event keyword
                    // Parse optional parameter: a key/cycle/instrument number,
                    // a multi-number list (`ON INTR 7,1`), ALL (`ON KBD ALL`),
                    // or a TIME expression (`ON TIME (TIMEDATE+X) MOD 86400`).
                    let param = if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
                        let mut nums = vec![self.expect_integer()?.to_string()];
                        // More integers follow only if the comma leads to
                        // another number; `ON DELAY 3, GOTO` has no second
                        // number before the response keyword.
                        while matches!(self.current.kind, TokenKind::Comma) {
                            self.advance();
                            if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
                                nums.push(self.expect_integer()?.to_string());
                            } else {
                                break;
                            }
                        }
                        Some(nums.join(","))
                    } else if let TokenKind::IoPath(ref name) = self.current.kind {
                        // `ON END @File GOTO Here` (on end.bas).
                        let n = name.clone();
                        self.advance();
                        Some(format!("@{}", n))
                    } else if let TokenKind::Identifier(ref id) = self.current.kind {
                        if id.to_uppercase() == "ALL" {
                            self.advance();
                            Some("ALL".to_string())
                        } else {
                            None
                        }
                    } else if kw == "TIME"
                        && !matches!(
                            self.current.kind,
                            TokenKind::GoTo
                                | TokenKind::GoSub
                                | TokenKind::Call
                                | TokenKind::Newline
                                | TokenKind::Eof
                                | TokenKind::Colon
                                | TokenKind::Semicolon
                        )
                    {
                        // Expression parameter; capture its source text.
                        let start = self.current.span.start;
                        let _ = self.parse_expression()?;
                        let end = self.current.span.start;
                        Some(
                            self.source[start.min(self.source.len())..end.min(self.source.len())]
                                .trim()
                                .to_string(),
                        )
                    } else {
                        None
                    };
                    // `ON DELAY 3, GOTO Here` — optional comma after the
                    // event parameter.
                    self.skip_comma();
                    // `ON KEY 1,LABEL "text",CALL name` — softkey label form.
                    if let TokenKind::Identifier(ref id) = self.current.kind {
                        if id.to_uppercase() == "LABEL" {
                            self.advance();
                            let text = self.expect_string()?;
                            self.skip_comma();
                            let response = match &self.current.kind {
                                TokenKind::GoTo => {
                                    self.advance();
                                    "GOTO".to_string()
                                },
                                TokenKind::GoSub => {
                                    self.advance();
                                    "GOSUB".to_string()
                                },
                                TokenKind::Call => {
                                    self.advance();
                                    "CALL".to_string()
                                },
                                _ => self.expect_identifier()?.to_uppercase(),
                            };
                            let label = self.expect_identifier_or_integer()?;
                            let event_key = if let Some(ref p) = param {
                                format!("ON {} {}", kw, p)
                            } else {
                                format!("ON {}", kw)
                            };
                            return Ok(Stmt::Configure(
                                event_key,
                                format!("LABEL {} {} {}", text, response, label),
                                span,
                            ));
                        }
                    }
                    // Parse response: GOTO/GOSUB/CALL label (or RECOVER)
                    let response = match &self.current.kind {
                        TokenKind::GoTo => {
                            self.advance();
                            "GOTO".to_string()
                        },
                        TokenKind::GoSub => {
                            self.advance();
                            "GOSUB".to_string()
                        },
                        TokenKind::Call => {
                            self.advance();
                            "CALL".to_string()
                        },
                        _ => self.expect_identifier()?.to_uppercase(),
                    };
                    let label = self.expect_identifier_or_integer()?;
                    let event_key = if let Some(ref p) = param {
                        format!("ON {} {}", kw, p)
                    } else {
                        format!("ON {}", kw)
                    };
                    Ok(Stmt::Configure(
                        event_key,
                        format!("{} {}", response, label),
                        span,
                    ))
                } else {
                    self.parse_on_computed(span)
                }
            } else {
                self.parse_on_computed(span)
            }
        }
    }

    // ===================== I/O Parsing =====================

    fn parse_print(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // PRINT

        // Check for USING
        if matches!(self.current.kind, TokenKind::PrintUsing) {
            self.advance();
            let format = self.parse_expression()?;

            let mut exprs = Vec::new();
            if matches!(self.current.kind, TokenKind::Semicolon) {
                self.advance();
                loop {
                    exprs.push(self.parse_expression()?);
                    if matches!(self.current.kind, TokenKind::Comma)
                        || matches!(self.current.kind, TokenKind::Semicolon)
                    {
                        self.advance();
                        if matches!(
                            self.current.kind,
                            TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                        ) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }

            return Ok(Stmt::PrintUsing(format, exprs, span));
        }

        // Parse standard PRINT items
        let mut items = Vec::new();

        loop {
            if matches!(
                self.current.kind,
                TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
            ) {
                break;
            }

            if matches!(self.current.kind, TokenKind::Semicolon) {
                items.push(PrintItem::Semicolon);
                self.advance();
                if matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    break;
                }
                continue;
            }

            if matches!(self.current.kind, TokenKind::Comma) {
                items.push(PrintItem::Comma);
                self.advance();
                continue;
            }

            // Try TAB(expr)
            if matches!(self.current.kind, TokenKind::Identifier(_)) {
                // Could be TAB function — but it's parsed as an expression
                let expr = self.parse_expression()?;
                items.push(PrintItem::Expr(expr));

                // Handle trailing comma/semicolon
                if matches!(self.current.kind, TokenKind::Comma) {
                    items.push(PrintItem::Comma);
                    self.advance();
                } else if matches!(self.current.kind, TokenKind::Semicolon) {
                    items.push(PrintItem::Semicolon);
                    self.advance();
                }
                continue;
            }

            // Default: parse expression
            let expr = self.parse_expression()?;
            items.push(PrintItem::Expr(expr));

            // Handle trailing comma/semicolon
            if matches!(self.current.kind, TokenKind::Comma) {
                items.push(PrintItem::Comma);
                self.advance();
            } else if matches!(self.current.kind, TokenKind::Semicolon) {
                items.push(PrintItem::Semicolon);
                self.advance();
            } else {
                break;
            }
        }

        Ok(Stmt::Print(items, span))
    }

    fn parse_print_using(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // PRINT USING

        // `PRINT USING Image; Price` — the image may be a reference to an
        // IMAGE statement, lexed as the Image keyword rather than an
        // identifier. Capture its source text as the format operand.
        let format = if matches!(self.current.kind, TokenKind::Image) {
            let start = self.current.span.start;
            self.advance();
            Expr::String_(
                self.source[start.min(self.source.len())..self.current.span.start.min(self.source.len())]
                    .trim()
                    .to_string(),
                span,
            )
        } else {
            self.parse_expression()?
        };
        let mut exprs = Vec::new();

        if matches!(self.current.kind, TokenKind::Semicolon) {
            self.advance();
            loop {
                if matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    break;
                }
                exprs.push(self.parse_expression()?);
                if matches!(self.current.kind, TokenKind::Comma)
                    || matches!(self.current.kind, TokenKind::Semicolon)
                {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        Ok(Stmt::PrintUsing(format, exprs, span))
    }

    fn parse_input(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // INPUT

        // Optional prompt string
        let prompt = if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
            let s = self.expect_string()?;
            // `,` also appears after INPUT/LINPUT prompts in converted files.
            if matches!(self.current.kind, TokenKind::Semicolon | TokenKind::Comma) {
                self.advance();
            }
            Some(s)
        } else {
            None
        };

        let mut vars = Vec::new();
        loop {
            vars.push(self.expect_identifier()?);
            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(Stmt::Input(prompt, vars, span))
    }

    fn parse_linput(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // LINPUT

        let prompt = if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
            let s = self.expect_string()?;
            // `,` also appears after INPUT/LINPUT prompts in converted files.
            if matches!(self.current.kind, TokenKind::Semicolon | TokenKind::Comma) {
                self.advance();
            }
            Some(s)
        } else {
            None
        };

        let var = self.expect_identifier()?;
        Ok(Stmt::Linput(prompt, var, span))
    }

    fn parse_read(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // READ

        let mut vars = Vec::new();
        loop {
            vars.push(self.expect_identifier()?);
            // Whole-array read: READ A(*)
            if matches!(self.current.kind, TokenKind::LParen) {
                self.advance();
                if matches!(self.current.kind, TokenKind::Star) {
                    self.advance();
                    self.expect(TokenKind::RParen)?;
                } else {
                    // Indexed read target (uncommon): keep the name, skip
                    // the index expression(s).
                    while !matches!(self.current.kind, TokenKind::RParen) {
                        self.advance();
                    }
                    self.advance(); // )
                }
            }
            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(Stmt::Read(vars, span))
    }

    fn parse_data(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // DATA

        let mut values = Vec::new();
        loop {
            if matches!(
                self.current.kind,
                TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
            ) {
                break;
            }
            let expr = self.parse_expression()?;
            values.push(expr);
            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }

        Ok(Stmt::Data(values, span))
    }

    fn parse_restore(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // RESTORE

        let label = if !matches!(
            self.current.kind,
            TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
        ) {
            Some(self.expect_identifier_or_integer()?)
        } else {
            None
        };

        Ok(Stmt::Restore(label, span))
    }

    fn parse_assign(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // ASSIGN

        // ASSIGN @name TO "filename" | DEVICE addr | BUFFER size
        if matches!(self.current.kind, TokenKind::IoPath(_)) {
            // Get the name from the IoPath token
            let path = match &self.current.kind {
                TokenKind::IoPath(ref name) => {
                    let n = name.clone();
                    self.advance();
                    n
                },
                _ => self.expect_identifier()?,
            };
            // Skip optional TO keyword (either as keyword or identifier)
            if matches!(self.current.kind, TokenKind::To) {
                self.advance();
            } else if let TokenKind::Identifier(ref id) = self.current.kind {
                if id.to_uppercase() == "TO" {
                    self.advance();
                }
            }
            // Parse destination: string, number, identifier, or the `*`
            // wildcard (`ASSIGN @Out TO *`).
            let mut dest = if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                self.expect_string()?
            } else if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
                self.expect_integer()?.to_string()
            } else if matches!(self.current.kind, TokenKind::Star) {
                self.advance();
                "*".to_string()
            } else {
                self.expect_identifier()?
            };
            // Trailing options: `ASSIGN @Out TO *; FORMAT ON` / `; EOL OFF`.
            while matches!(self.current.kind, TokenKind::Semicolon) {
                self.advance();
                let opt = self.expect_identifier()?;
                let state = if matches!(self.current.kind, TokenKind::Identifier(_)) {
                    format!("; {} {}", opt, self.expect_identifier()?)
                } else if matches!(self.current.kind, TokenKind::On) {
                    self.advance();
                    format!("; {opt} ON")
                } else {
                    format!("; {opt}")
                };
                dest = format!("{dest}{state}");
            }
            Ok(Stmt::Configure(format!("ASSIGN @{}", path), dest, span))
        } else {
            // Fallback — skip rest of line
            while !matches!(
                self.current.kind,
                TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
            ) {
                self.advance();
            }
            Ok(Stmt::Rem("ASSIGN".into(), span))
        }
    }

    fn parse_output(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // OUTPUT
                        // OUTPUT @path; expr, expr, ... — the path is an IoPath token, not
        // an Identifier, and `;` or `,` may follow it.
        let path = match &self.current.kind {
            TokenKind::IoPath(name) => {
                let n = name.clone();
                self.advance();
                Some(n)
            },
            TokenKind::Identifier(name) => {
                let n = name.clone();
                self.advance();
                Some(n)
            },
            _ => None,
        };
        // Skip `;` or `,` after the path
        if matches!(self.current.kind, TokenKind::Semicolon | TokenKind::Comma) {
            self.advance();
        }
        // Print-item list, separators preserved: a trailing `;` or `,`
        // suppresses the line terminator (handled at runtime).
        let mut items = Vec::new();
        while !matches!(
            self.current.kind,
            TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
        ) {
            if matches!(self.current.kind, TokenKind::Semicolon) {
                self.advance();
                items.push(PrintItem::Semicolon);
            } else if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
                items.push(PrintItem::Comma);
            } else {
                items.push(PrintItem::Expr(self.parse_expression()?));
            }
        }
        Ok(Stmt::Output(path.unwrap_or_default(), items, span))
    }

    fn parse_enter_stmt(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // ENTER
        let path = match &self.current.kind {
            TokenKind::IoPath(name) => {
                let n = name.clone();
                self.advance();
                Some(n)
            },
            TokenKind::Identifier(name) => {
                let n = name.clone();
                self.advance();
                Some(n)
            },
            // Numeric select code: `ENTER 9; X` (on timeout.bas).
            TokenKind::IntegerLiteral(n) => {
                let s = n.to_string();
                self.advance();
                Some(s)
            },
            _ => None,
        };
        if matches!(self.current.kind, TokenKind::Semicolon | TokenKind::Comma) {
            self.advance();
        }
        let vars = if !matches!(self.current.kind, TokenKind::Newline | TokenKind::Eof) {
            vec![self.expect_identifier()?]
        } else {
            vec![]
        };
        Ok(Stmt::Configure(
            format!("ENTER @{}", path.unwrap_or_default()),
            vars.first().cloned().unwrap_or_default(),
            span,
        ))
    }

    fn parse_call(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // CALL

        // CALL "name" — string-literal subprogram name (DLL-style calls).
        let name = match &self.current.kind {
            TokenKind::StringLiteral(s) => {
                let n = s.clone();
                self.advance();
                n
            },
            _ => self.expect_identifier()?,
        };
        let mut args = Vec::new();

        if matches!(self.current.kind, TokenKind::LParen) {
            self.advance();
            loop {
                if matches!(self.current.kind, TokenKind::RParen) {
                    break;
                }

                // Handle special argument types
                if matches!(self.current.kind, TokenKind::At) {
                    self.advance();
                    let io_name = self.expect_identifier()?;
                    args.push(Expr::Variable(format!("@{}", io_name), self.span()));
                } else if matches!(self.current.kind, TokenKind::Star) {
                    self.advance();
                    // Array pass: (*) or just *
                    if matches!(self.current.kind, TokenKind::RParen) {
                        self.advance();
                    }
                    args.push(Expr::Variable("*".into(), self.span()));
                } else {
                    args.push(self.parse_expression()?);
                }

                if matches!(self.current.kind, TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
        }

        // `CALL "Msg", WITH("Line three",3)` — pass-by-value argument list.
        if matches!(self.current.kind, TokenKind::Comma) {
            self.advance();
            if let TokenKind::Identifier(ref id) = self.current.kind {
                if id.to_uppercase() == "WITH" {
                    self.advance();
                    if matches!(self.current.kind, TokenKind::LParen) {
                        self.advance();
                        while !matches!(self.current.kind, TokenKind::RParen | TokenKind::Eof) {
                            let _ = self.parse_expression()?;
                            if matches!(self.current.kind, TokenKind::Comma) {
                                self.advance();
                            }
                        }
                        self.expect(TokenKind::RParen)?;
                    }
                } else {
                    args.push(self.parse_expression()?);
                }
            } else {
                args.push(self.parse_expression()?);
            }
        }

        Ok(Stmt::Call(name, args, span))
    }

    // ===================== Matrix Parsing =====================

    fn parse_mat(&mut self) -> Result<Stmt> {
        let span = self.span();
        self.advance(); // MAT

        // MAT INPUT, MAT PRINT, MAT READ
        if matches!(self.current.kind, TokenKind::Input_) {
            self.advance();
            let name = self.expect_identifier()?;
            return Ok(Stmt::Mat(MatOp::Input(name, span), span));
        }
        if matches!(self.current.kind, TokenKind::Print) {
            self.advance();
            let name = self.expect_identifier()?;
            return Ok(Stmt::Mat(MatOp::Print(name, span), span));
        }
        if matches!(self.current.kind, TokenKind::Read) {
            self.advance();
            let name = self.expect_identifier()?;
            return Ok(Stmt::Mat(MatOp::Read(name, span), span));
        }

        // Reduction / reorder forms: MAT SORT A(*), MAT REORDER M BY V,n,
        // MAT CSUM A, MAT RSUM A (no `=` involved). These operate in place.
        if let TokenKind::Identifier(ref kw) = self.current.kind {
            let upper = kw.to_uppercase();
            if matches!(upper.as_str(), "SORT" | "REORDER" | "CSUM" | "RSUM") {
                self.advance();
                let src = self.expect_identifier()?;
                // BY vector (MAT REORDER M BY V,n) or TO vector (MAT SORT A
                // TO V). `TO` lexes as a keyword, `BY` as an identifier.
                let at_sep = |parser: &Parser| {
                    matches!(parser.current.kind, TokenKind::To)
                        || matches!(&parser.current.kind, TokenKind::Identifier(ref id)
                            if id.eq_ignore_ascii_case("TO") || id.eq_ignore_ascii_case("BY"))
                };
                let mut vector = None;
                // The whole-array marker may sit between the array and the
                // separator (`MAT SORT B(*) TO B(1)` — mat sort.prg).
                if !at_sep(self) {
                    if matches!(self.current.kind, TokenKind::LParen) {
                        self.advance();
                        if matches!(self.current.kind, TokenKind::Star) {
                            self.advance();
                        }
                        self.expect(TokenKind::RParen)?;
                    }
                }
                if at_sep(self) {
                    self.advance();
                    vector = Some(self.expect_identifier()?);
                    // Vector slot marker: MAT SORT B(*) TO B(1) (the `(1)`
                    // is usually suppressed by the converter, but tolerate).
                    if matches!(self.current.kind, TokenKind::LParen) {
                        self.advance();
                        if matches!(self.current.kind, TokenKind::IntegerLiteral(_)) {
                            self.advance();
                        }
                        self.expect(TokenKind::RParen)?;
                    }
                }
                // Optional subscript: REORDER M BY V,2
                let mut subscript = None;
                if matches!(self.current.kind, TokenKind::Comma) {
                    self.advance();
                    if let TokenKind::IntegerLiteral(ref n) = self.current.kind {
                        subscript = Some(*n);
                        self.advance();
                    }
                }
                // Trailing direction modifier: MAT SORT A(*) DESC
                // (mat sort.prg). DES is the abbreviated spelling.
                let mut desc = false;
                if let TokenKind::Identifier(ref m) = self.current.kind {
                    if matches!(m.to_uppercase().as_str(), "DESC" | "DES") {
                        desc = true;
                        self.advance();
                    }
                }
                let func = match upper.as_str() {
                    "RSUM" => ReducFunc::Rsum,
                    "CSUM" => ReducFunc::Csum,
                    "SORT" if desc => ReducFunc::SortDesc,
                    "SORT" => ReducFunc::Sort,
                    _ => ReducFunc::Reorder,
                };
                return Ok(Stmt::Mat(
                    MatOp::Reduc(src.clone(), func, src, vector, subscript, span),
                    span,
                ));
            }
        }

        // MAT A = ...
        let dest = self.expect_identifier()?;
        self.expect(TokenKind::Eq)?;

        // Check for MAT functions: INV, TRN, ZER, CON, IDN, RSUM, CSUM
        let mat_func = match &self.current.kind {
            TokenKind::Identifier(name) => {
                let upper = name.to_uppercase();
                match upper.as_str() {
                    "INV" => Some(MatFunc::Inv),
                    "TRN" => Some(MatFunc::Trn),
                    "ZER" => Some(MatFunc::Zer),
                    "CON" => Some(MatFunc::Con),
                    "IDN" => Some(MatFunc::Idn),
                    _ => None,
                }
            },
            _ => None,
        };

        // Assignment-form reductions: `MAT Vector=CSUM(Matrix)` (csum.bas),
        // `MAT V=RSUM(M)`.
        if let TokenKind::Identifier(ref id) = self.current.kind {
            let red = match id.to_uppercase().as_str() {
                "CSUM" => Some(ReducFunc::Csum),
                "RSUM" => Some(ReducFunc::Rsum),
                _ => None,
            };
            if let Some(red) = red {
                self.advance();
                let src = if matches!(self.current.kind, TokenKind::LParen) {
                    self.advance();
                    let n = self.expect_identifier()?;
                    if !matches!(self.current.kind, TokenKind::RParen | TokenKind::RBracket) {
                        return Err(HtBasicError::ParseError {
                            expected: ")".into(),
                            found: self.current.kind.name().into(),
                            span: self.span(),
                        });
                    }
                    self.advance();
                    n
                } else {
                    self.expect_identifier()?
                };
                return Ok(Stmt::Mat(
                    MatOp::Reduc(dest, red, src, None, None, span),
                    span,
                ));
            }
        }

        if let Some(func) = mat_func {
            self.advance(); // consume function name
                            // Check for argument: FUNC(src) or FUNC with no args
            if matches!(self.current.kind, TokenKind::LParen) {
                self.advance();
                // Could be FUNC(src) or FUNC(dim1, dim2, ...)
                // Check if next token is an identifier (array name) or an integer (dimension)
                let first = self.parse_expression()?;
                if matches!(self.current.kind, TokenKind::Comma) {
                    // Dimension list: IDN(3,3), ZER(2,2)
                    let mut dims = vec![];
                    // First expression is a dimension bound
                    // Parse dim list
                    dims.push((0, first.as_integer_or_zero()));
                    while matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                        let dim_expr = self.parse_expression()?;
                        dims.push((0, dim_expr.as_integer_or_zero()));
                    }
                    self.expect(TokenKind::RParen)?;
                    Ok(Stmt::Mat(MatOp::FuncInit(dest, func, dims, span), span))
                } else {
                    // Single argument: TRN(A)
                    self.expect(TokenKind::RParen)?;
                    // First is an expression that evaluates to an array name
                    let src_name = match &first {
                        Expr::Variable(name, _) => name.clone(),
                        _ => String::new(),
                    };
                    Ok(Stmt::Mat(MatOp::Func(dest, func, src_name, span), span))
                }
            } else {
                // No arguments: MAT A = ZER, MAT A = CON, MAT A = IDN
                Ok(Stmt::Mat(MatOp::FuncInit(dest, func, vec![], span), span))
            }
        } else if matches!(self.current.kind, TokenKind::LParen) {
            // `MAT B$=("E")` — parenthesized initializer; represent as an
            // empty ZER init (parse-clean stand-in).
            self.advance();
            let _ = self.parse_expression()?;
            self.expect(TokenKind::RParen)?;
            Ok(Stmt::Mat(MatOp::FuncInit(dest, MatFunc::Zer, vec![], span), span))
        } else {
            let src = self.expect_identifier()?;

            // Check for binary operator
            if matches!(
                self.current.kind,
                TokenKind::Plus
                    | TokenKind::Minus
                    | TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::Dot
            ) {
                let op = match self.current.kind {
                    TokenKind::Plus => MatBinOp::Add,
                    TokenKind::Minus => MatBinOp::Sub,
                    TokenKind::Star => MatBinOp::Mul,
                    TokenKind::Slash => MatBinOp::Div,
                    TokenKind::Dot => MatBinOp::DotMul,
                    _ => MatBinOp::Add,
                };
                self.advance();
                let src2 = self.expect_identifier()?;
                Ok(Stmt::Mat(MatOp::Binary(dest, src, op, src2, span), span))
            } else {
                // Simple assignment: MAT A = B
                Ok(Stmt::Mat(MatOp::Assign(dest, src, span), span))
            }
        }
    }

    // ===================== Graphics Command Parsing =====================

    fn parse_graphics_cmd(&mut self, cmd: &str) -> Result<Stmt> {
        self.advance(); // consume the keyword
        let span = self.span();

        match cmd {
            "GINIT" => Ok(Stmt::Gfx(GfxCmd::Ginit, span)),
            "GCLEAR" => Ok(Stmt::Gfx(GfxCmd::Gclear, span)),
            "PENUP" => Ok(Stmt::Gfx(GfxCmd::Penup, span)),
            "FRAME" => Ok(Stmt::Gfx(GfxCmd::Frame, span)),
            "CLIP" => {
                if let TokenKind::Identifier(ref id) = self.current.kind {
                    if id.to_uppercase() == "OFF" {
                        self.advance();
                        return Ok(Stmt::Gfx(GfxCmd::ClipOff, span));
                    }
                }
                let x1 = self.parse_number()?;
                let y1 = self.parse_number()?;
                let x2 = self.parse_number()?;
                let y2 = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Clip(x1, y1, x2, y2), span))
            },
            "WINDOW" => {
                let x1 = self.parse_number()?;
                let x2 = self.parse_number()?;
                let y1 = self.parse_number()?;
                let y2 = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Window(x1, x2, y1, y2), span))
            },
            "VIEWPORT" => {
                let x1 = self.parse_number()?;
                let x2 = self.parse_number()?;
                let y1 = self.parse_number()?;
                let y2 = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Viewport(x1, x2, y1, y2), span))
            },
            "MOVE" => {
                let x = self.parse_number()?;
                self.skip_comma();
                let y = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Move(x, y), span))
            },
            "DRAW" => {
                let x = self.parse_number()?;
                self.skip_comma();
                let y = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Draw(x, y, false, false), span))
            },
            "IMOVE" => {
                let dx = self.parse_number()?;
                let dy = self.parse_number()?;
                // IMOVE is relative — we'll handle by adding to current position at runtime
                Ok(Stmt::Gfx(GfxCmd::Draw(dx, dy, true, false), span))
            },
            "IDRAW" => {
                let dx = self.parse_number()?;
                let dy = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Draw(dx, dy, true, true), span))
            },
            "PLOT" => {
                let x = self.parse_number()?;
                let y = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Plot(x, y), span))
            },
            "PEN" => {
                let n = self.parse_number()? as usize;
                Ok(Stmt::Gfx(GfxCmd::Pen(n), span))
            },
            "LABEL" => {
                // LABEL "text" — or LABEL <expression> (ai.bas: LABEL Loop).
                let s = if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                    self.expect_string()?
                } else {
                    let start = self.current.span.start;
                    let _ = self.parse_expression()?;
                    let end = self.current.span.start;
                    self.source[start.min(self.source.len())..end.min(self.source.len())]
                        .trim()
                        .to_string()
                };
                Ok(Stmt::Gfx(GfxCmd::Label(s), span))
            },
            "CSIZE" => {
                let w = self.parse_number()?;
                let h = if !matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    Some(self.parse_number()?)
                } else {
                    None
                };
                Ok(Stmt::Gfx(GfxCmd::Csize(w, h), span))
            },
            "LDIR" => {
                let angle = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::Ldir(angle), span))
            },
            "LORG" => {
                let n = self.parse_number()? as usize;
                Ok(Stmt::Gfx(GfxCmd::Lorg(n), span))
            },
            "GFONT" => {
                // Optional IS: GFONT [IS] "name"
                if let TokenKind::Identifier(ref id) = self.current.kind {
                    if id.to_uppercase() == "IS" {
                        self.advance();
                    }
                }
                let s = self.expect_string()?;
                Ok(Stmt::Gfx(GfxCmd::Gfont(s), span))
            },
            "AXES" => {
                // 0–4 optional arguments (defaults 0.0).
                let xtic = self.parse_optional_number()?;
                let ytic = self.parse_optional_number()?;
                let xorg = self.parse_optional_number()?;
                let yorg = self.parse_optional_number()?;
                Ok(Stmt::Gfx(GfxCmd::Axes(xtic, ytic, xorg, yorg), span))
            },
            "GRID" => {
                // 0–4 optional arguments (defaults 0.0).
                let xtic = self.parse_optional_number()?;
                let ytic = self.parse_optional_number()?;
                let xorg = self.parse_optional_number()?;
                let yorg = self.parse_optional_number()?;
                Ok(Stmt::Gfx(GfxCmd::Grid(xtic, ytic, xorg, yorg), span))
            },
            "GLOAD" => {
                if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                    let s = self.expect_string()?;
                    Ok(Stmt::Gfx(GfxCmd::Gload(s), span))
                } else {
                    // Device form: GLOAD CRT,3;A(*)
                    let device = match &self.current.kind {
                        TokenKind::IoPath(n) => {
                            let n = n.clone();
                            self.advance();
                            format!("@{n}")
                        },
                        TokenKind::Identifier(n) => {
                            let n = n.clone();
                            self.advance();
                            n
                        },
                        _ => String::new(),
                    };
                    let mut spec = device;
                    if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                        let n = self.parse_number()?;
                        spec = format!("{spec},{n}");
                    }
                    if matches!(self.current.kind, TokenKind::Semicolon) {
                        self.advance();
                        spec = format!("{spec};{}", self.expect_identifier()?);
                        if matches!(self.current.kind, TokenKind::LParen) {
                            self.advance();
                            if matches!(self.current.kind, TokenKind::Star) {
                                self.advance();
                            }
                            self.expect(TokenKind::RParen)?;
                            spec = format!("{spec}(*)");
                        }
                    }
                    Ok(Stmt::Gfx(GfxCmd::Gload(spec), span))
                }
            },
            "GSTORE" => {
                if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                    let s = self.expect_string()?;
                    Ok(Stmt::Gfx(GfxCmd::Gstore(s), span))
                } else {
                    // Device form: GSTORE CRT,1;A(*)
                    let device = match &self.current.kind {
                        TokenKind::IoPath(n) => {
                            let n = n.clone();
                            self.advance();
                            format!("@{n}")
                        },
                        TokenKind::Identifier(n) => {
                            let n = n.clone();
                            self.advance();
                            n
                        },
                        _ => String::new(),
                    };
                    let mut spec = device;
                    if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                        let n = self.parse_number()?;
                        spec = format!("{spec},{n}");
                    }
                    if matches!(self.current.kind, TokenKind::Semicolon) {
                        self.advance();
                        spec = format!("{spec};{}", self.expect_identifier()?);
                        if matches!(self.current.kind, TokenKind::LParen) {
                            self.advance();
                            if matches!(self.current.kind, TokenKind::Star) {
                                self.advance();
                            }
                            self.expect(TokenKind::RParen)?;
                            spec = format!("{spec}(*)");
                        }
                    }
                    Ok(Stmt::Gfx(GfxCmd::Gstore(spec), span))
                }
            },
            "RECTANGLE" => {
                let w = self.parse_number()?;
                let h = self.parse_number()?;
                let (fill, edge) = self.parse_fill_edge();
                Ok(Stmt::Gfx(GfxCmd::Rectangle(w, h, fill, edge), span))
            },
            "COLOR" => {
                // COLOR "name" — or COLOR MAP (identifier specifier).
                let s = if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                    self.expect_string()?
                } else {
                    self.expect_identifier()?
                };
                Ok(Stmt::Gfx(GfxCmd::Color(s), span))
            },
            "SEPARATE" => {
                let next = self.expect_identifier()?.to_uppercase();
                if next == "ALPHA" {
                    Ok(Stmt::Gfx(GfxCmd::SeparateAlpha, span))
                } else {
                    Ok(Stmt::Rem(format!("SEPARATE {}", next), span))
                }
            },
            "MERGE" => {
                let next = self.expect_identifier()?.to_uppercase();
                if next == "ALPHA" {
                    Ok(Stmt::Gfx(GfxCmd::MergeAlpha, span))
                } else {
                    Ok(Stmt::Rem(format!("MERGE {}", next), span))
                }
            },
            "LINE" => {
                let next = self.expect_identifier()?.to_uppercase();
                if next == "TYPE" {
                    let n = self.parse_number()? as usize;
                    Ok(Stmt::Gfx(GfxCmd::LineType(n), span))
                } else {
                    Ok(Stmt::Rem(format!("LINE {}", next), span))
                }
            },
            "AREA" => {
                let next = self.expect_identifier()?.to_uppercase();
                if next == "PEN" {
                    let n = self.parse_number()? as usize;
                    Ok(Stmt::Gfx(GfxCmd::Pen(n), span))
                } else if next == "COLOR" {
                    let h = self.parse_number()?;
                    let s = self.parse_number()?;
                    let l = self.parse_number()?;
                    Ok(Stmt::Gfx(GfxCmd::AreaColor(h, s, l), span))
                } else if next == "INTENSITY" {
                    let r = self.parse_number()?;
                    let g = self.parse_number()?;
                    let b = self.parse_number()?;
                    Ok(Stmt::Gfx(GfxCmd::AreaIntensity(r, g, b), span))
                } else {
                    Ok(Stmt::Rem("AREA".into(), span))
                }
            },
            "INTENSITY" => {
                let r = self.parse_number()?;
                let g = self.parse_number()?;
                let b = self.parse_number()?;
                Ok(Stmt::Gfx(GfxCmd::IntEnsity(r, g, b), span))
            },
            "SET" => {
                let next = self.expect_identifier()?.to_uppercase();
                if next == "PEN" {
                    let n = self.parse_number()? as usize;
                    // SET PEN n INTENSITY r,g,b
                    if let TokenKind::Identifier(ref id) = self.current.kind {
                        if id.to_uppercase() == "INTENSITY" {
                            self.advance();
                            let r = self.parse_number()?;
                            let g = self.parse_number()?;
                            let b = self.parse_number()?;
                            return Ok(Stmt::Gfx(GfxCmd::SetPen(n, r, g, b), span));
                        }
                    }
                    Ok(Stmt::Gfx(GfxCmd::Pen(n), span))
                } else {
                    Ok(Stmt::Rem("SET".into(), span))
                }
            },
            "PLOTTER" => {
                let next = self.expect_identifier()?.to_uppercase();
                if next == "IS" {
                    let device = self.expect_identifier()?;
                    let mut options = String::new();
                    if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                        if matches!(self.current.kind, TokenKind::StringLiteral(_)) {
                            options = self.expect_string()?;
                        } else {
                            options = self.expect_identifier()?;
                        }
                    }
                    Ok(Stmt::Gfx(GfxCmd::PlotterIs(device, options), span))
                } else {
                    Ok(Stmt::Rem("PLOTTER".into(), span))
                }
            },
            "POLYGON" => {
                let radius = self.parse_number()?;
                let chords = self.parse_polygon_chords()?;
                let (fill, edge) = self.parse_fill_edge();
                Ok(Stmt::Gfx(GfxCmd::PolygonReg(radius, chords, fill, edge), span))
            },
            "POLYLINE" => {
                let radius = self.parse_number()?;
                let chords = self.parse_polygon_chords()?;
                Ok(Stmt::Gfx(GfxCmd::PolylineReg(radius, chords), span))
            },
            "DIGITIZE" => Ok(Stmt::Gfx(GfxCmd::DigiTize, span)),
            "READ" => {
                if let TokenKind::Identifier(ref id) = self.current.kind {
                    if id.to_uppercase() == "LOCATOR" {
                        self.advance();
                        let var = self.expect_identifier()?;
                        return Ok(Stmt::Gfx(GfxCmd::ReadLocator(var), span));
                    }
                }
                // Fall through to parse_read
                self.parse_read()
            },
            _ => Err(HtBasicError::ParseError {
                expected: "graphics command".into(),
                found: cmd.to_string(),
                span,
            }),
        }
    }

    fn skip_comma(&mut self) {
        if matches!(self.current.kind, TokenKind::Comma) {
            self.advance();
        }
    }

    /// Number argument that defaults to 0.0 when the next token cannot
    /// start one (`AXES`/`GRID` take 0–4 arguments).
    fn parse_optional_number(&mut self) -> Result<f64> {
        let next_is_number = matches!(
            self.current.kind,
            TokenKind::IntegerLiteral(_)
                | TokenKind::RealLiteral(_)
                | TokenKind::Minus
                | TokenKind::Identifier(_)
                | TokenKind::StringIdentifier(_)
                | TokenKind::LParen
        );
        if next_is_number {
            self.parse_number()
        } else {
            Ok(0.0)
        }
    }

    /// Optional `total[,drawn]` chord counts after a POLYGON/POLYLINE
    /// radius (e.g. `POLYGON 10,10,8`). Drawn defaults to total; both
    /// default to 60 in the interpreter when absent here.
    fn parse_polygon_chords(&mut self) -> Result<Option<(f64, f64)>> {
        let next_is_number = matches!(
            self.current.kind,
            TokenKind::IntegerLiteral(_) | TokenKind::RealLiteral(_) | TokenKind::Minus
        );
        if !next_is_number {
            return Ok(None);
        }
        let total = self.parse_number()?;
        let drawn = if matches!(
            self.current.kind,
            TokenKind::IntegerLiteral(_) | TokenKind::RealLiteral(_) | TokenKind::Minus
        ) {
            self.parse_number()?
        } else {
            total
        };
        Ok(Some((total, drawn)))
    }

    /// Optional FILL/EDGE flags after a POLYGON/RECTANGLE area specifier.
    /// The preceding parse_number already consumed the comma before the
    /// first flag; "comma flag" pairs may repeat. If neither is given,
    /// EDGE is assumed (HTBasic default).
    fn parse_fill_edge(&mut self) -> (bool, bool) {
        let mut fill = false;
        let mut edge = false;
        loop {
            let kw = match &self.current.kind {
                TokenKind::Identifier(name) => name.to_uppercase(),
                _ => break,
            };
            if kw == "FILL" {
                fill = true;
            } else if kw == "EDGE" {
                edge = true;
            } else {
                break;
            }
            self.advance();
            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            }
        }
        if !fill && !edge {
            edge = true;
        }
        (fill, edge)
    }

    fn parse_point_list(&mut self) -> Result<Vec<(f64, f64)>> {
        let mut points = Vec::new();
        loop {
            if matches!(
                self.current.kind,
                TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
            ) {
                break;
            }
            let x = self.parse_number()?;
            let y = self.parse_number()?;
            points.push((x, y));
            if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(points)
    }

    fn parse_number(&mut self) -> Result<f64> {
        let val = match &self.current.kind {
            TokenKind::IntegerLiteral(n) => {
                let val = *n as f64;
                self.advance();
                val
            },
            TokenKind::RealLiteral(n) => {
                let val = *n;
                self.advance();
                val
            },
            TokenKind::Minus => {
                self.advance();
                -self.parse_number()?
            },
            _ => {
                let expr = self.parse_expression()?;
                match expr {
                    Expr::Integer(n, _) => n as f64,
                    Expr::Real(n, _) => n,
                    _ => 0.0,
                }
            },
        };
        // Auto-skip trailing comma for convenience in arg lists
        self.skip_comma();
        Ok(val)
    }

    // ===================== Expression Parsing (Pratt) =====================

    fn parse_expression(&mut self) -> Result<Expr> {
        self.parse_pratt(Precedence::Lowest)
    }

    fn parse_pratt(&mut self, min_prec: Precedence) -> Result<Expr> {
        let mut left = self.parse_prefix()?;

        loop {
            // Check for binary operators
            let op = match &self.current.kind {
                TokenKind::Plus => Some(BinaryOp::Add),
                TokenKind::Minus => Some(BinaryOp::Sub),
                TokenKind::Star => Some(BinaryOp::Mul),
                TokenKind::Slash => Some(BinaryOp::Div),
                TokenKind::Caret => Some(BinaryOp::Pow),
                TokenKind::Amp => Some(BinaryOp::Concat),
                TokenKind::Eq => Some(BinaryOp::Eq),
                TokenKind::LtGt => Some(BinaryOp::NotEq),
                TokenKind::Lt => Some(BinaryOp::Lt),
                TokenKind::Gt => Some(BinaryOp::Gt),
                TokenKind::LtEq => Some(BinaryOp::LtEq),
                TokenKind::GtEq => Some(BinaryOp::GtEq),
                TokenKind::And => Some(BinaryOp::And),
                TokenKind::Or => Some(BinaryOp::Or),
                TokenKind::Exor => Some(BinaryOp::Exor),
                TokenKind::Mod_ => Some(BinaryOp::Mod_),
                TokenKind::Modulo => Some(BinaryOp::Modulo),
                TokenKind::Div_ => Some(BinaryOp::Div_),
                _ => None,
            };

            if let Some(op) = op {
                let (left_prec, right_prec) = binary_precedence(&op);
                if left_prec < min_prec {
                    break;
                }
                self.advance();
                let right = self.parse_pratt(right_prec)?;
                let span = Span::merge(
                    match &left {
                        Expr::Integer(_, s) => *s,
                        Expr::Real(_, s) => *s,
                        Expr::String_(_, s) => *s,
                        Expr::Variable(_, s) => *s,
                        Expr::StringVariable(_, s) => *s,
                        Expr::ArrayRef(_, _, s) => *s,
                        Expr::WholeArray(_, s) => *s,
                        Expr::FnCall(_, _, s) => *s,
                        Expr::StringFnCall(_, _, s) => *s,
                        Expr::SubStr(_, _, _, _, s) => *s,
                        Expr::Unary(_, _, s) => *s,
                        Expr::Binary(_, _, _, s) => *s,
                    },
                    match &right {
                        Expr::Integer(_, s) => *s,
                        Expr::Real(_, s) => *s,
                        Expr::String_(_, s) => *s,
                        Expr::Variable(_, s) => *s,
                        Expr::StringVariable(_, s) => *s,
                        Expr::ArrayRef(_, _, s) => *s,
                        Expr::WholeArray(_, s) => *s,
                        Expr::FnCall(_, _, s) => *s,
                        Expr::StringFnCall(_, _, s) => *s,
                        Expr::SubStr(_, _, _, _, s) => *s,
                        Expr::Unary(_, _, s) => *s,
                        Expr::Binary(_, _, _, s) => *s,
                    },
                );
                left = Expr::Binary(Box::new(left), op, Box::new(right), span);
                continue;
            }

            // Check for substring: A$[start, end] or A$[start; length]
            if matches!(self.current.kind, TokenKind::LBracket) {
                // Only valid after a string variable or string expression
                if matches!(
                    left,
                    Expr::StringVariable(_, _) | Expr::StringFnCall(_, _, _)
                ) {
                    self.advance(); // [
                    let start = self.parse_expression()?;

                    let (end, is_length) = if matches!(self.current.kind, TokenKind::Semicolon) {
                        self.advance();
                        (Some(Box::new(self.parse_expression()?)), true)
                    } else if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                        (Some(Box::new(self.parse_expression()?)), false)
                    } else {
                        (None, false)
                    };

                    let span = self.span();
                    self.expect(TokenKind::RBracket)?;

                    let name = match &left {
                        Expr::StringVariable(n, _) => n.clone(),
                        _ => String::new(),
                    };
                    left = Expr::SubStr(name, Box::new(start), end, is_length, span);
                    continue;
                }
            }

            break;
        }

        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr> {
        let span = self.span();

        match &self.current.kind {
            TokenKind::IntegerLiteral(n) => {
                let val = *n;
                self.advance();
                Ok(Expr::Integer(val, span))
            },
            TokenKind::RealLiteral(n) => {
                let val = *n;
                self.advance();
                Ok(Expr::Real(val, span))
            },
            TokenKind::Real => {
                // REAL doubles as a type keyword and a conversion function;
                // in expression position it is the function.
                self.advance();
                if matches!(self.current.kind, TokenKind::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    loop {
                        if matches!(self.current.kind, TokenKind::RParen) {
                            break;
                        }
                        // Slice range separators in array refs: `C$(1,:4,*)`
                        // (dim.bas) — skip leading colons and fold `*`
                        // whole-range markers into the arg list.
                        while matches!(self.current.kind, TokenKind::Colon) {
                            self.advance();
                        }
                        if matches!(self.current.kind, TokenKind::Star) {
                            self.advance();
                            args.push(Expr::String_("*".into(), span));
                            if matches!(self.current.kind, TokenKind::Comma | TokenKind::Colon) {
                                self.advance();
                            }
                            continue;
                        }
                        args.push(self.parse_expression()?);
                        if matches!(self.current.kind, TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    return Ok(Expr::FnCall("REAL".to_string(), args, span));
                }
                Ok(Expr::Variable("REAL".to_string(), span))
            },
            TokenKind::StringLiteral(s) => {
                let val = s.clone();
                self.advance();
                Ok(Expr::String_(val, span))
            },
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();

                // Function call: name(args)
                if matches!(self.current.kind, TokenKind::LParen) {
                    self.advance();
                    // Whole-array reference: A(*) — passed to SUBs and
                    // used with READ/PRINT.
                    if matches!(self.current.kind, TokenKind::Star) {
                        self.advance();
                        self.expect(TokenKind::RParen)?;
                        return Ok(Expr::WholeArray(name, span));
                    }
                    let mut args = Vec::new();
                    loop {
                        if matches!(self.current.kind, TokenKind::RParen) {
                            break;
                        }
                        // Slice range separators in array refs: `C$(1,:4,*)`
                        // (dim.bas) — skip leading colons and fold `*`
                        // whole-range markers into the arg list.
                        while matches!(self.current.kind, TokenKind::Colon) {
                            self.advance();
                        }
                        if matches!(self.current.kind, TokenKind::Star) {
                            self.advance();
                            args.push(Expr::String_("*".into(), span));
                            if matches!(self.current.kind, TokenKind::Comma | TokenKind::Colon) {
                                self.advance();
                            }
                            continue;
                        }
                        args.push(self.parse_expression()?);
                        if matches!(self.current.kind, TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen)?;

                    // Substring of function result? e.g., FNname$(args)[1,3]
                    // Handle that in the Pratt loop
                    return Ok(Expr::FnCall(name, args, span));
                }

                Ok(Expr::Variable(name, span))
            },
            TokenKind::StringIdentifier(name) => {
                let name = name.clone();
                self.advance();

                // String function call: name$(args)
                if matches!(self.current.kind, TokenKind::LParen) {
                    self.advance();
                    // Whole-array reference: A$(*)
                    if matches!(self.current.kind, TokenKind::Star) {
                        self.advance();
                        self.expect(TokenKind::RParen)?;
                        return Ok(Expr::WholeArray(name, span));
                    }
                    let mut args = Vec::new();
                    loop {
                        if matches!(self.current.kind, TokenKind::RParen) {
                            break;
                        }
                        // Slice range separators in array refs: `C$(1,:4,*)`
                        // (dim.bas) — skip leading colons and fold `*`
                        // whole-range markers into the arg list.
                        while matches!(self.current.kind, TokenKind::Colon) {
                            self.advance();
                        }
                        if matches!(self.current.kind, TokenKind::Star) {
                            self.advance();
                            args.push(Expr::String_("*".into(), span));
                            if matches!(self.current.kind, TokenKind::Comma | TokenKind::Colon) {
                                self.advance();
                            }
                            continue;
                        }
                        args.push(self.parse_expression()?);
                        if matches!(self.current.kind, TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    return Ok(Expr::StringFnCall(name, args, span));
                }

                Ok(Expr::StringVariable(name, span))
            },
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            },
            // Unary operators
            TokenKind::Minus => {
                self.advance();
                let right = self.parse_pratt(Precedence::Unary)?;
                let rspan = Span::merge(
                    span,
                    match &right {
                        Expr::Integer(_, s) => *s,
                        Expr::Real(_, s) => *s,
                        Expr::String_(_, s) => *s,
                        Expr::Variable(_, s) => *s,
                        Expr::StringVariable(_, s) => *s,
                        Expr::ArrayRef(_, _, s) => *s,
                        Expr::WholeArray(_, s) => *s,
                        Expr::FnCall(_, _, s) => *s,
                        Expr::StringFnCall(_, _, s) => *s,
                        Expr::SubStr(_, _, _, _, s) => *s,
                        Expr::Unary(_, _, s) => *s,
                        Expr::Binary(_, _, _, s) => *s,
                    },
                );
                Ok(Expr::Unary(UnaryOp::Minus, Box::new(right), rspan))
            },
            TokenKind::Plus => {
                self.advance();
                let right = self.parse_pratt(Precedence::Unary)?;
                let rspan = Span::merge(
                    span,
                    match &right {
                        Expr::Integer(_, s) => *s,
                        Expr::Real(_, s) => *s,
                        _ => span,
                    },
                );
                Ok(Expr::Unary(UnaryOp::Plus, Box::new(right), rspan))
            },
            TokenKind::Not => {
                self.advance();
                let right = self.parse_pratt(Precedence::Not)?;
                let rspan = Span::merge(
                    span,
                    match &right {
                        Expr::Integer(_, s) => *s,
                        Expr::Real(_, s) => *s,
                        Expr::String_(_, s) => *s,
                        Expr::Variable(_, s) => *s,
                        Expr::StringVariable(_, s) => *s,
                        Expr::ArrayRef(_, _, s) => *s,
                        Expr::WholeArray(_, s) => *s,
                        Expr::FnCall(_, _, s) => *s,
                        Expr::StringFnCall(_, _, s) => *s,
                        Expr::SubStr(_, _, _, _, s) => *s,
                        Expr::Unary(_, _, s) => *s,
                        Expr::Binary(_, _, _, s) => *s,
                    },
                );
                Ok(Expr::Unary(UnaryOp::Not, Box::new(right), rspan))
            },
            _ => Err(HtBasicError::ParseError {
                expected: "expression".into(),
                found: self.current.kind.name().into(),
                span,
            }),
        }
    }

    // ===================== Helper Parsers =====================

    fn expect_identifier(&mut self) -> Result<String> {
        match &self.current.kind {
            TokenKind::Identifier(name) | TokenKind::StringIdentifier(name) => {
                let n = name.clone();
                self.advance();
                Ok(n)
            },
            _ => Err(HtBasicError::ParseError {
                expected: "identifier".into(),
                found: self.current.kind.name().into(),
                span: self.span(),
            }),
        }
    }

    fn expect_identifier_or_string(&mut self) -> Result<String> {
        match &self.current.kind {
            TokenKind::Identifier(name) | TokenKind::StringIdentifier(name) => {
                let n = name.clone();
                self.advance();
                Ok(n)
            },
            _ => Err(HtBasicError::ParseError {
                expected: "identifier".into(),
                found: self.current.kind.name().into(),
                span: self.span(),
            }),
        }
    }

    fn expect_identifier_or_integer(&mut self) -> Result<String> {
        match &self.current.kind {
            TokenKind::Identifier(name) | TokenKind::StringIdentifier(name) => {
                let n = name.clone();
                self.advance();
                Ok(n)
            },
            TokenKind::IntegerLiteral(n) => {
                let s = n.to_string();
                self.advance();
                Ok(s)
            },
            // Word-like keyword tokens are legal label names
            // (`GOTO End` — end select.bas).
            TokenKind::End => {
                self.advance();
                Ok("End".to_string())
            },
            _ => Err(HtBasicError::ParseError {
                expected: "identifier or line number".into(),
                found: self.current.kind.name().into(),
                span: self.span(),
            }),
        }
    }

    fn expect_string(&mut self) -> Result<String> {
        match &self.current.kind {
            TokenKind::StringLiteral(s) => {
                let val = s.clone();
                self.advance();
                Ok(val)
            },
            _ => Err(HtBasicError::ParseError {
                expected: "string literal".into(),
                found: self.current.kind.name().into(),
                span: self.span(),
            }),
        }
    }

    fn expect_integer(&mut self) -> Result<i64> {
        match &self.current.kind {
            TokenKind::IntegerLiteral(n) => {
                let val = *n;
                self.advance();
                Ok(val)
            },
            _ => Err(HtBasicError::ParseError {
                expected: "integer".into(),
                found: self.current.kind.name().into(),
                span: self.span(),
            }),
        }
    }

    fn parse_block_until(&mut self, end_tokens: &[TokenKind]) -> Result<Vec<Stmt>> {
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            // `340 END WHILE` — line number precedes the terminator.
            if let Some(lbl) = self.consume_labeled_kw(end_tokens) {
                body.push(lbl);
            }
            if matches!(self.current.kind, TokenKind::Eof) {
                break;
            }
            let mut should_break = false;
            for end in end_tokens {
                if std::mem::discriminant(&self.current.kind) == std::mem::discriminant(end) {
                    should_break = true;
                    break;
                }
            }
            if should_break {
                break;
            }
            let stmts = self.parse_statement_or_line()?;
            body.extend(stmts);
        }
        Ok(body)
    }

    fn parse_implicit_let(&mut self) -> Result<Stmt> {
        // We've already checked for LET keyword, this handles the rest:
        // variable = expression
        let name = self.expect_identifier()?;

        // Check for array ref: A(1,2) = ...
        if matches!(self.current.kind, TokenKind::LParen) {
            self.advance(); // (
            let mut indices = Vec::new();
            loop {
                if matches!(self.current.kind, TokenKind::RParen) {
                    break;
                }
                // Slice markers: `LET A$(1,:4,*)=...` (dim.bas) uses `:`
                // ranges and `*` whole-range. A range separator can lead
                // the next index (`1,:4`), so skip leading colons.
                while matches!(self.current.kind, TokenKind::Colon) {
                    self.advance();
                }
                if matches!(self.current.kind, TokenKind::Star) {
                    self.advance();
                    indices.push(Expr::String_("*".into(), self.span()));
                    if matches!(self.current.kind, TokenKind::Comma | TokenKind::Colon) {
                        self.advance();
                    }
                    continue;
                }
                indices.push(self.parse_expression()?);
                if matches!(self.current.kind, TokenKind::Comma | TokenKind::Colon) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::Eq)?;
            let value = self.parse_expression()?;
            let span = self.span();
            return Ok(Stmt::ArrayAssign(name, indices, value, span));
        }

        // Check for substring assignment: A$[1,2] = ...
        if matches!(self.current.kind, TokenKind::LBracket) && name.ends_with('$') {
            self.advance(); // [
            let start = self.parse_expression()?;
            let end = if matches!(self.current.kind, TokenKind::Comma) {
                self.advance();
                Some(self.parse_expression()?)
            } else if matches!(self.current.kind, TokenKind::Semicolon) {
                self.advance();
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.expect(TokenKind::RBracket)?;
            self.expect(TokenKind::Eq)?;
            let value = self.parse_expression()?;
            let span = self.span();
            return Ok(Stmt::SubStrAssign(
                name,
                start,
                end.unwrap_or_else(|| Expr::Integer(0, span)),
                value,
                span,
            ));
        }

        self.expect(TokenKind::Eq)?;
        let expr = self.parse_expression()?;
        Ok(Stmt::Let(name, expr, self.span()))
    }

    fn parse_assignment_with_target(&mut self, _target: Expr) -> Result<Stmt> {
        // For array and complex assignments — simplified for now
        while !matches!(
            self.current.kind,
            TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
        ) {
            self.advance();
        }
        Ok(Stmt::Rem("assignment".into(), Span::new(0, 0)))
    }

    /// Parse a statement that starts with an identifier or unknown token.
    /// Handles implicit LET, or single-token statements.
    fn parse_implicit_or_expression_stmt(&mut self) -> Result<Stmt> {
        match &self.current.kind {
            TokenKind::Identifier(name) | TokenKind::StringIdentifier(name) => {
                let name = name.clone();
                let span = self.span();

                // Save the name and check for =, (, or [
                self.advance();

                // Check if it's an assignment
                if matches!(self.current.kind, TokenKind::Eq) {
                    self.advance();
                    let expr = self.parse_expression()?;
                    return Ok(Stmt::Let(name, expr, span));
                }

                // Check for array reference: A(1), A(1,2), etc.
                if matches!(self.current.kind, TokenKind::LParen) {
                    self.advance(); // consume (
                    let mut indices = Vec::new();
                    loop {
                        if matches!(self.current.kind, TokenKind::RParen) {
                            break;
                        }
                        // Slice markers: `A$(1,:4,*)` (dim.bas) uses `:`
                        // ranges and `*` whole-range; `Palette(*)` is a
                        // whole-array leftover (set pen.bas). A range
                        // separator can lead the next index (`1,:4`),
                        // so skip leading colons.
                        while matches!(self.current.kind, TokenKind::Colon) {
                            self.advance();
                        }
                        if matches!(self.current.kind, TokenKind::Star) {
                            self.advance();
                            if matches!(self.current.kind, TokenKind::RParen) {
                                self.advance();
                                return Ok(Stmt::Rem(format!("whole_array:{}", name), span));
                            }
                            indices.push(Expr::String_("*".into(), span));
                            if matches!(self.current.kind, TokenKind::Comma | TokenKind::Colon) {
                                self.advance();
                            }
                            continue;
                        }
                        indices.push(self.parse_expression()?);
                        if matches!(self.current.kind, TokenKind::Comma | TokenKind::Colon) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen)?;

                    if matches!(self.current.kind, TokenKind::Eq) {
                        self.advance();
                        let value = self.parse_expression()?;
                        return Ok(Stmt::ArrayAssign(name, indices, value, span));
                    }
                    // Not an assignment — a bare `Name(args)` statement is a
                    // CALL without the CALL keyword (`Prtmat(Matrix(*),3,3)`
                    // — csum.bas).
                    return Ok(Stmt::Call(name, indices, span));
                }

                // Check for substring assignment
                if matches!(self.current.kind, TokenKind::LBracket) && name.ends_with('$') {
                    self.advance();
                    let start = self.parse_expression()?;
                    let end = if matches!(self.current.kind, TokenKind::Comma) {
                        self.advance();
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };
                    self.expect(TokenKind::RBracket)?;

                    if matches!(self.current.kind, TokenKind::Eq) {
                        self.advance();
                        let value = self.parse_expression()?;
                        return Ok(Stmt::SubStrAssign(
                            name,
                            start,
                            end.unwrap_or_else(|| Expr::Integer(1, span)),
                            value,
                            span,
                        ));
                    }
                }

                // Just a bare identifier — could be a CALL without the CALL keyword
                Ok(Stmt::Call(name, vec![], span))
            },
            TokenKind::Return => {
                // Handle "RETURN" as both keyword and bare statement
                let span = self.span();
                self.advance();
                let expr = if !matches!(
                    self.current.kind,
                    TokenKind::Newline | TokenKind::Eof | TokenKind::Colon
                ) {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Ok(Stmt::Return(expr, span))
            },
            _ => {
                // Skip unknown token
                let tok = self.current.clone();
                self.advance();
                Ok(Stmt::Rem(format!("unknown:{}", tok.kind.name()), tok.span))
            },
        }
    }

    // ===================== Subprogram / Function Parsing =====================

    fn parse_subprogram(&mut self) -> Result<SubProgram> {
        let span = self.span();
        self.advance(); // SUB

        let name = self.expect_identifier()?;
        let mut params = Vec::new();

        if matches!(self.current.kind, TokenKind::LParen) {
            self.advance();
            loop {
                if matches!(self.current.kind, TokenKind::RParen) {
                    break;
                }

                // `SUB Bigparams(A, B, OPTIONAL C, D)` — OPTIONAL is a
                // call-convention hint, not a type.
                if let TokenKind::Identifier(ref id) = self.current.kind {
                    if id.to_uppercase() == "OPTIONAL" {
                        self.advance();
                    }
                }

                let param_type = if matches!(self.current.kind, TokenKind::At) {
                    self.advance();
                    ParamType::IoPath
                } else if matches!(self.current.kind, TokenKind::Star) {
                    self.advance();
                    ParamType::Array
                } else if matches!(self.current.kind, TokenKind::Redim) {
                    // `SUB Pass_a(REDIM A(*))` — whole-array parameter.
                    self.advance();
                    ParamType::Array
                } else if matches!(
                    self.current.kind,
                    TokenKind::Integer
                        | TokenKind::Real
                        | TokenKind::Short
                        | TokenKind::Long
                        | TokenKind::Complex
                ) {
                    // Typed parameter: `SUB See(INTEGER X)`.
                    self.advance();
                    ParamType::Variable
                } else {
                    ParamType::Variable
                };

                let param_name = self.expect_identifier()?;

                // Check for (*) array pass
                if matches!(self.current.kind, TokenKind::LParen) {
                    self.advance();
                    self.expect(TokenKind::Star)?;
                    self.expect(TokenKind::RParen)?;
                    params.push(Param {
                        name: param_name,
                        param_type: ParamType::Array,
                    });
                } else {
                    if param_name.ends_with('$') {
                        params.push(Param {
                            name: param_name,
                            param_type: ParamType::String_,
                        });
                    } else {
                        params.push(Param {
                            name: param_name,
                            param_type,
                        });
                    }
                }

                if matches!(self.current.kind, TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
        }

        self.skip_newlines();

        // Parse body until SUBEND
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            // `300 SUBEND` — line number precedes SUBEND.
            if let Some(lbl) = self.consume_labeled_kw(&[TokenKind::SubEnd]) {
                body.push(lbl);
            }
            if matches!(self.current.kind, TokenKind::SubEnd | TokenKind::Eof) {
                break;
            }
            let stmts = self.parse_statement_or_line()?;
            body.extend(stmts);
        }

        if matches!(self.current.kind, TokenKind::SubEnd) {
            self.advance();
        }

        Ok(SubProgram {
            name,
            params,
            body,
            span,
        })
    }

    fn parse_fn_def(&mut self) -> Result<FnDef> {
        let span = self.span();
        self.advance(); // DEF FN

        let raw_name = self.expect_identifier()?;
        // Prepend "FN" since DEF FN strips it during lexing
        let name = format!("FN{}", raw_name);
        let returns_string = name.ends_with('$');

        let mut params = Vec::new();
        let mut required_params = 0;
        let mut seen_optional = false;

        if matches!(self.current.kind, TokenKind::LParen) {
            self.advance();
            loop {
                if matches!(self.current.kind, TokenKind::RParen) {
                    break;
                }
                // `DEF FNMessage$(OPTIONAL String$)` — params to the right of
                // OPTIONAL need not be passed (def fn.prg, fn.prg).
                if let TokenKind::Identifier(ref id) = self.current.kind {
                    if id.eq_ignore_ascii_case("OPTIONAL") {
                        self.advance();
                        seen_optional = true;
                    }
                }
                let param_name = self.expect_identifier()?;
                let pt = if param_name.ends_with('$') {
                    ParamType::String_
                } else {
                    ParamType::Variable
                };
                params.push(Param {
                    name: param_name,
                    param_type: pt,
                });
                if !seen_optional {
                    required_params += 1;
                }

                if matches!(self.current.kind, TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
        }

        self.skip_newlines();

        // Parse body until FNEND
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            // `540 FNEND` — line number precedes FNEND.
            if let Some(lbl) = self.consume_labeled_kw(&[TokenKind::FnEnd]) {
                body.push(lbl);
            }
            if matches!(self.current.kind, TokenKind::FnEnd | TokenKind::Eof) {
                break;
            }
            let stmts = self.parse_statement_or_line()?;
            body.extend(stmts);
        }

        if matches!(self.current.kind, TokenKind::FnEnd) {
            self.advance();
        }

        Ok(FnDef {
            name,
            returns_string,
            params,
            required_params,
            body,
            span,
        })
    }
}
