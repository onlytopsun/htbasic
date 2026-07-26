#[allow(dead_code)]
// Symbol table and scope management.
//
// HTBasic has these scope levels (outermost to innermost):
// 1. Global scope — variables declared before END
// 2. COM blocks — shared memory blocks
// 3. SUB/FN local scope — parameters and local variables
//
// Currently implemented inline in the interpreter for simplicity.
// This module will be expanded in Phase 2+.
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub symbol_type: SymbolType,
    pub is_array: bool,
    pub dimensions: Vec<(i64, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolType {
    Real,
    Integer,
    Short,
    Long,
    Complex,
    String_,
    Unknown,
}

#[derive(Debug)]
pub struct SymbolTable {
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn define(&mut self, name: &str, info: SymbolInfo) {
        let upper = name.to_uppercase();
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(upper, info);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&SymbolInfo> {
        let upper = name.to_uppercase();
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(&upper) {
                return Some(info);
            }
        }
        None
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
