#[allow(dead_code)]
/// Name resolution pass — stub for future semantic analysis.
use crate::parser::ast::Program;

pub struct Resolver;

impl Resolver {
    pub fn new() -> Self {
        Self
    }
    pub fn resolve(&mut self, _program: &Program) -> crate::error::Result<()> {
        Ok(())
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}
