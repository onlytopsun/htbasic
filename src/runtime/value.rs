use std::fmt;
use std::rc::Rc;

/// Runtime value for HTBasic.
///
/// Uses `Rc<str>` for strings to make cloning O(1) (reference count increment)
/// instead of O(n) heap allocation. This is important because BASIC passes
/// values by copy frequently.
#[derive(Clone, PartialEq)]
pub enum Value {
    Real(f64),
    Integer(i64),
    String_(Rc<str>),
    Array(ArrayData),
    Null,
}

impl Value {
    /// Create a string value from a &str.
    pub fn string(s: &str) -> Self {
        Value::String_(Rc::from(s))
    }

    /// Convert to a display string for PRINT.
    pub fn to_display_string(&self) -> String {
        match self {
            Value::Real(n) => {
                // Format real numbers without trailing zeros
                let s = format!("{:.15}", n);
                let s = s.trim_end_matches('0');
                let s = s.trim_end_matches('.');
                if s.is_empty() || s == "-" {
                    "0".to_string()
                } else {
                    s.to_string()
                }
            },
            Value::Integer(n) => n.to_string(),
            Value::String_(s) => s.to_string(),
            Value::Array(arr) => {
                // Row-major listing: `PRINT A(*)` → `12 15 18`.
                arr.data
                    .iter()
                    .map(|v| v.to_display_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            },
            Value::Null => "".to_string(),
        }
    }

    /// Coerce to a numeric value (Real preferred, Integer if exact).
    pub fn as_real(&self) -> f64 {
        match self {
            Value::Real(n) => *n,
            Value::Integer(n) => *n as f64,
            Value::String_(s) => s.parse::<f64>().unwrap_or(0.0),
            Value::Null => 0.0,
            Value::Array(_) => 0.0,
        }
    }

    /// Coerce to an integer.
    pub fn as_integer(&self) -> i64 {
        match self {
            Value::Integer(n) => *n,
            Value::Real(n) => *n as i64,
            Value::String_(s) => s.parse::<i64>().unwrap_or(0),
            Value::Null => 0,
            Value::Array(_) => 0,
        }
    }

    /// Coerce to a string.
    pub fn as_string(&self) -> String {
        match self {
            Value::String_(s) => s.to_string(),
            Value::Real(n) => n.to_string(),
            Value::Integer(n) => n.to_string(),
            Value::Null => "".to_string(),
            Value::Array(_) => "Array".to_string(),
        }
    }

    /// Check if the value is truthy (non-zero for numbers, non-empty for strings).
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Real(n) => *n != 0.0,
            Value::Integer(n) => *n != 0,
            Value::String_(s) => !s.is_empty(),
            Value::Null => false,
            Value::Array(_) => true,
        }
    }

    /// Type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Real(_) => "REAL",
            Value::Integer(_) => "INTEGER",
            Value::String_(_) => "STRING",
            Value::Array(_) => "ARRAY",
            Value::Null => "NULL",
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Real(n) => write!(f, "Real({})", n),
            Value::Integer(n) => write!(f, "Integer({})", n),
            Value::String_(s) => write!(f, "String({:?})", s),
            Value::Array(a) => write!(f, "Array({:?})", a),
            Value::Null => write!(f, "Null"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

/// Multi-dimensional array storage.
#[derive(Clone, PartialEq, Debug)]
pub struct ArrayData {
    /// Dimensions: vector of (lower_bound, upper_bound) pairs.
    pub dims: Vec<(i64, i64)>,
    /// Flat row-major storage.
    pub data: Vec<Value>,
}

impl ArrayData {
    /// Create a new array with the given dimension bounds.
    /// Filled with Null values.
    pub fn new(dims: Vec<(i64, i64)>) -> Self {
        let total: usize = dims
            .iter()
            .map(|&(lo, hi)| (hi - lo + 1).max(0) as usize)
            .product();
        Self {
            dims,
            data: vec![Value::Null; total],
        }
    }

    /// Compute the flat index for the given subscripts.
    pub fn index(&self, subscripts: &[i64]) -> Option<usize> {
        if subscripts.len() != self.dims.len() {
            return None;
        }

        let mut flat_idx = 0usize;
        let mut stride = 1usize;

        // Row-major: last dimension varies fastest
        for i in (0..self.dims.len()).rev() {
            let (lo, hi) = self.dims[i];
            let sub = subscripts[i];
            if sub < lo || sub > hi {
                return None;
            }
            flat_idx += (sub - lo) as usize * stride;
            stride *= (hi - lo + 1) as usize;
        }

        Some(flat_idx)
    }

    /// Get a value at the given subscripts.
    pub fn get(&self, subscripts: &[i64]) -> Option<&Value> {
        self.index(subscripts).map(|i| &self.data[i])
    }

    /// Set a value at the given subscripts.
    pub fn set(&mut self, subscripts: &[i64], value: Value) -> bool {
        if let Some(i) = self.index(subscripts) {
            self.data[i] = value;
            true
        } else {
            false
        }
    }

    /// Total number of elements.
    pub fn total_elements(&self) -> usize {
        self.data.len()
    }
}
