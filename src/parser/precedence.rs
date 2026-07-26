#[allow(dead_code)]

/// Operator precedence for HTBasic's Pratt parser.
///
/// HTBasic precedence (highest to lowest):
/// 1. () []
/// 2. Functions
/// 3. ^ (exponentiation)
/// 4. * / DIV MODULO MOD
/// 5. Unary + -  (BELOW exponentiation — unusual!)
/// 6. Binary + -
/// 7. & (string concatenation)
/// 8. = <> < > <= >= (relational)
/// 9. NOT
/// 10. AND
/// 11. OR EXOR
///
/// Higher binding power = tighter binding.
/// Same-priority operators are left-associative.

/// Precedence levels (higher = binds tighter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precedence {
    Lowest = 0,
    Logical = 1,    // OR, EXOR
    And = 2,        // AND
    Not = 3,        // NOT
    Comparison = 4, // = <> < > <= >=
    Concat = 5,     // &
    Sum = 6,        // + -
    Product = 7,    // * / DIV MODULO MOD
    Unary = 8,      // Unary + - (note: BELOW exponentiation in HTBasic)
    Power = 9,      // ^
    Call = 10,      // Function calls
    Primary = 11,   // Literals, variables, parens
}

/// Binary operator to precedence mapping.
pub fn binary_precedence(op: &super::ast::BinaryOp) -> (Precedence, Precedence) {
    // Returns (left_binding_power, right_binding_power)
    // For left-assoc: left == right; for right-assoc: right = left - 1
    use super::ast::BinaryOp::*;
    let p = match op {
        Or | Exor => Precedence::Logical,
        And => Precedence::And,
        Eq | NotEq | Lt | Gt | LtEq | GtEq => Precedence::Comparison,
        Concat => Precedence::Concat,
        Add | Sub => Precedence::Sum,
        Mul | Div | Mod_ | Modulo | Div_ => Precedence::Product,
        Pow => Precedence::Power,
    };
    // All binary operators are left-associative in HTBasic
    (p, p)
}
