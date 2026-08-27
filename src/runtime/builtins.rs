use crate::runtime::value::{ArrayData, Value};
use std::cell::Cell;

/// Registry of built-in functions.
pub struct Builtins {
    functions: Vec<(&'static str, usize, fn(&[Value]) -> Value)>,
}

impl Builtins {
    pub fn new() -> Self {
        let mut b = Self {
            functions: Vec::new(),
        };
        b.register_all();
        b
    }

    fn register(&mut self, name: &'static str, arg_count: usize, func: fn(&[Value]) -> Value) {
        self.functions.push((name, arg_count, func));
    }

    /// Look up a built-in function by name (case-insensitive).
    /// When `provided_args` is Some(n), matches the exact arg count.
    pub fn get_with_args(
        &self,
        name: &str,
        provided_args: usize,
    ) -> Option<(usize, fn(&[Value]) -> Value)> {
        let upper = name.to_uppercase();
        let clean = upper.trim_end_matches('$');
        for (n, argc, func) in &self.functions {
            if (*n == clean || *n == upper.as_str()) && *argc == provided_args {
                return Some((*argc, *func));
            }
        }
        None
    }

    /// Look up a built-in function by name (case-insensitive).
    /// Returns the first matching function regardless of arg count.
    pub fn get(&self, name: &str) -> Option<(usize, fn(&[Value]) -> Value)> {
        let upper = name.to_uppercase();
        let clean = upper.trim_end_matches('$');
        // Try exact match first, then fall back to first by name
        for (n, argc, func) in &self.functions {
            if *n == clean || *n == upper.as_str() {
                return Some((*argc, *func));
            }
        }
        None
    }

    /// Check if a name is a built-in function.
    pub fn exists(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Seed the random number generator.
    pub fn randomize(&self, seed: f64) {
        RNG_SEED.with(|s| s.set((seed as u64).max(1)));
    }

    fn register_all(&mut self) {
        // --- Math functions ---
        self.register("ABS", 1, |args| match &args[0] {
            Value::Real(n) => Value::Real(n.abs()),
            Value::Integer(n) => Value::Integer(n.abs()),
            _ => Value::Real(args[0].as_real().abs()),
        });

        self.register("SQR", 1, |args| Value::Real(args[0].as_real().sqrt()));

        self.register("SIN", 1, |args| Value::Real(args[0].as_real().sin()));

        self.register("COS", 1, |args| Value::Real(args[0].as_real().cos()));

        self.register("TAN", 1, |args| Value::Real(args[0].as_real().tan()));

        self.register("ATN", 1, |args| Value::Real(args[0].as_real().atan()));

        self.register("ASIN", 1, |args| {
            let x = args[0].as_real();
            Value::Real(x.asin())
        });

        self.register("ACOS", 1, |args| {
            let x = args[0].as_real();
            Value::Real(x.acos())
        });

        self.register("EXP", 1, |args| Value::Real(args[0].as_real().exp()));

        self.register("LOG", 1, |args| Value::Real(args[0].as_real().ln()));

        self.register("LOG10", 1, |args| Value::Real(args[0].as_real().log10()));

        self.register("INT", 1, |args| {
            Value::Integer(args[0].as_real().trunc() as i64)
        });

        self.register("FRACT", 1, |args| {
            let x = args[0].as_real();
            Value::Real(x - x.trunc())
        });

        self.register("CEIL", 1, |args| Value::Real(args[0].as_real().ceil()));

        self.register("FLOOR", 1, |args| Value::Real(args[0].as_real().floor()));

        self.register("ROUND", 2, |args| {
            let x = args[0].as_real();
            let digits = args[1].as_integer();
            let factor = 10_f64.powi(digits as i32);
            Value::Real((x * factor).round() / factor)
        });

        self.register("SGN", 1, |args| {
            let x = args[0].as_real();
            Value::Integer(if x > 0.0 {
                1
            } else if x < 0.0 {
                -1
            } else {
                0
            })
        });

        self.register("MAX", 2, |args| {
            let a = args[0].as_real();
            let b = args[1].as_real();
            Value::Real(a.max(b))
        });

        self.register("MIN", 2, |args| {
            let a = args[0].as_real();
            let b = args[1].as_real();
            Value::Real(a.min(b))
        });

        self.register("RND", 0, |_args| {
            // Simple PRNG using system time — HTBasic uses a deterministic PRNG
            // but for now we just return 0.0-1.0 random
            Value::Real(rand_fast())
        });

        self.register("PI", 0, |_args| Value::Real(std::f64::consts::PI));

        self.register("DEG", 1, |args| Value::Real(args[0].as_real().to_degrees()));

        self.register("RAD", 1, |args| Value::Real(args[0].as_real().to_radians()));

        // --- String functions ---
        self.register("LEN", 1, |args| {
            Value::Integer(args[0].as_string().len() as i64)
        });

        self.register("UPC$", 1, |args| {
            Value::string(&args[0].as_string().to_uppercase())
        });

        self.register("LWC$", 1, |args| {
            Value::string(&args[0].as_string().to_lowercase())
        });

        self.register("TRIM$", 1, |args| Value::string(args[0].as_string().trim()));

        self.register("LTRIM$", 1, |args| {
            Value::string(args[0].as_string().trim_start())
        });

        self.register("RTRIM$", 1, |args| {
            Value::string(args[0].as_string().trim_end())
        });

        self.register("REV$", 1, |args| {
            let s: String = args[0].as_string().chars().rev().collect();
            Value::string(&s)
        });

        self.register("RPT$", 2, |args| {
            let s = args[0].as_string();
            let n = args[1].as_integer().max(0) as usize;
            Value::string(&s.repeat(n))
        });

        self.register("CHR$", 1, |args| {
            let n = args[0].as_integer() as u8;
            Value::string(&String::from_utf8_lossy(&[n]))
        });

        self.register("STR$", 1, |args| {
            Value::string(&args[0].to_display_string())
        });

        self.register("VAL", 1, |args| {
            let s = args[0].as_string().trim().to_string();
            if let Ok(n) = s.parse::<f64>() {
                Value::Real(n)
            } else {
                Value::Real(0.0)
            }
        });

        self.register("VAL$", 1, |args| {
            Value::string(&args[0].to_display_string())
        });

        self.register("NUM", 1, |args| {
            let s = args[0].as_string();
            // Return ASCII value of first character
            Value::Integer(s.chars().next().map(|c| c as i64).unwrap_or(0))
        });

        self.register("POS", 2, |args| {
            let haystack = args[0].as_string();
            let needle = args[1].as_string();
            if let Some(pos) = haystack.find(&needle) {
                Value::Integer(pos as i64 + 1) // 1-based
            } else {
                Value::Integer(0)
            }
        });

        self.register("MAXLEN", 1, |args| {
            match &args[0] {
                Value::String_(s) => Value::Integer(s.len() as i64),
                _ => Value::Integer(18), // default max length
            }
        });

        // --- Type conversion ---
        self.register("REAL", 1, |args| Value::Real(args[0].as_real()));

        self.register("INTEGER", 1, |args| Value::Integer(args[0].as_integer()));

        // (CMPLX is registered below in the Complex section)

        // --- Hyperbolic functions ---
        self.register("SINH", 1, |args| Value::Real(args[0].as_real().sinh()));
        self.register("COSH", 1, |args| Value::Real(args[0].as_real().cosh()));
        self.register("TANH", 1, |args| Value::Real(args[0].as_real().tanh()));
        self.register("ASINH", 1, |args| Value::Real(args[0].as_real().asinh()));
        self.register("ACOSH", 1, |args| Value::Real(args[0].as_real().acosh()));
        self.register("ATANH", 1, |args| Value::Real(args[0].as_real().atanh()));

        // --- Additional math ---
        self.register("TRUNCATE", 1, |args| {
            Value::Integer(args[0].as_real().trunc() as i64)
        });
        self.register("ROUND", 1, |args| Value::Real(args[0].as_real().round()));
        self.register("ROUND", 2, |args| {
            let x = args[0].as_real();
            let p = 10_f64.powi(args[1].as_integer() as i32);
            Value::Real((x * p).round() / p)
        });
        self.register("RANDOMIZE", 0, |_args| {
            // RANDOMIZE without arg uses current time
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            RNG_SEED.with(|s| s.set(t.max(1)));
            Value::Real(0.0)
        });
        self.register("RANDOMIZE", 1, |args| {
            let seed = args[0].as_real() as u64;
            RNG_SEED.with(|s| s.set(seed.max(1)));
            Value::Real(0.0)
        });

        // --- String functions (extended) ---
        self.register("LTRIM$", 1, |args| {
            Value::string(args[0].as_string().trim_start())
        });
        self.register("RTRIM$", 1, |args| {
            Value::string(args[0].as_string().trim_end())
        });
        self.register("LCASE$", 1, |args| {
            Value::string(&args[0].as_string().to_lowercase())
        });
        self.register("UCASE$", 1, |args| {
            Value::string(&args[0].as_string().to_uppercase())
        });
        self.register("SPACE$", 1, |args| {
            let n = args[0].as_integer().max(0) as usize;
            Value::string(&" ".repeat(n))
        });
        self.register("STRING$", 2, |args| {
            let n = args[0].as_integer().max(0) as usize;
            let ch = if args[1].as_real() != 0.0 {
                (args[1].as_integer() as u8) as char
            } else {
                args[1].as_string().chars().next().unwrap_or(' ')
            };
            Value::string(&ch.to_string().repeat(n))
        });
        self.register("INSTR", 2, |args| {
            let haystack = args[1].as_string().to_uppercase();
            let needle = args[0].as_string().to_uppercase();
            haystack
                .find(&needle)
                .map(|p| Value::Integer(p as i64 + 1))
                .unwrap_or(Value::Integer(0))
        });
        self.register("INSTR", 3, |args| {
            let start = (args[0].as_integer() - 1).max(0) as usize;
            let full = args[2].as_string().to_uppercase();
            let haystack = if start < full.len() {
                &full[start..]
            } else {
                ""
            };
            let needle = args[1].as_string().to_uppercase();
            haystack
                .find(&needle)
                .map(|p| Value::Integer(p as i64 + start as i64 + 1))
                .unwrap_or(Value::Integer(0))
        });

        // --- Date/Time with real system time ---
        self.register("DATE$", 0, |_args| Value::string(&current_date_string()));
        self.register("TIME$", 0, |_args| Value::string(&current_time_string()));
        self.register("TIMEDATE", 0, |_args| {
            Value::Real(current_seconds_since_midnight())
        });

        // --- System functions ---
        self.register("SYSTEM$", 1, |args| {
            let query = args[0].as_string().to_uppercase();
            match query.as_str() {
                "VERSION:HTB" => Value::string("1.0"),
                "VERSION" => Value::string("HTBasic Interpreter 0.2.0"),
                _ => Value::string(""),
            }
        });
        self.register("ENVIRON$", 1, |args| {
            std::env::var(args[0].as_string())
                .map(|v| Value::string(&v))
                .unwrap_or(Value::string(""))
        });
        self.register("COMMAND$", 0, |_args| Value::string(""));

        // --- Bit operations ---
        self.register("BIT", 2, |args| {
            let val = args[0].as_integer() as u64;
            let bit = args[1].as_integer();
            Value::Integer(((val >> bit) & 1) as i64)
        });
        self.register("BINAND", 2, |args| {
            Value::Integer(args[0].as_integer() & args[1].as_integer())
        });
        self.register("BINOR", 2, |args| {
            Value::Integer(args[0].as_integer() | args[1].as_integer())
        });
        self.register("BINXOR", 2, |args| {
            Value::Integer(args[0].as_integer() ^ args[1].as_integer())
        });
        self.register("BINNOT", 1, |args| Value::Integer(!args[0].as_integer()));
        self.register("SHL", 2, |args| {
            Value::Integer(args[0].as_integer() << args[1].as_integer())
        });
        self.register("SHR", 2, |args| {
            Value::Integer(args[0].as_integer() >> args[1].as_integer())
        });

        // --- Error information placeholders ---
        self.register("ERRN", 0, |_args| Value::Integer(0));
        self.register("ERRL", 0, |_args| Value::Integer(0));
        self.register("ERRM$", 0, |_args| Value::string(""));

        // --- Complex number functions ---
        // Complex numbers are represented as two-element arrays or (real, imag) pairs.
        // CMPLX(real, imag) — create complex from two scalars
        self.register("CMPLX", 2, |args| {
            let vals = vec![
                Value::Real(args[0].as_real()),
                Value::Real(args[1].as_real()),
            ];
            Value::Array(ArrayData {
                dims: vec![(0, 1)],
                data: vals,
            })
        });
        // REAL(z) — return real part (also works on scalars)
        self.register("REAL", 1, |args| match &args[0] {
            Value::Array(arr) if arr.data.len() >= 1 => arr.data[0].clone(),
            v => Value::Real(v.as_real()),
        });
        // IMAG(z) — return imaginary part
        self.register("IMAG", 1, |args| match &args[0] {
            Value::Array(arr) if arr.data.len() >= 2 => arr.data[1].clone(),
            _ => Value::Real(0.0),
        });
        // CONJG(z) — complex conjugate
        self.register("CONJG", 1, |args| match &args[0] {
            Value::Array(ref arr) if arr.data.len() >= 2 => {
                let re = arr.data[0].as_real();
                let im = -arr.data[1].as_real();
                Value::Array(ArrayData {
                    dims: vec![(0, 1)],
                    data: vec![Value::Real(re), Value::Real(im)],
                })
            },
            v => v.clone(),
        });
        // ARG(z) — phase angle in radians
        self.register("ARG", 1, |args| {
            let re = match &args[0] {
                Value::Array(arr) if arr.data.len() >= 1 => arr.data[0].as_real(),
                v => v.as_real(),
            };
            let im = match &args[0] {
                Value::Array(arr) if arr.data.len() >= 2 => arr.data[1].as_real(),
                _ => 0.0,
            };
            Value::Real(im.atan2(re))
        });

        // --- Statistics functions ---
        // SUM(array) — sum of all elements
        self.register("SUM", 1, |args| match &args[0] {
            Value::Array(ref arr) => {
                let sum: f64 = arr.data.iter().map(|v| v.as_real()).sum();
                Value::Real(sum)
            },
            v => Value::Real(v.as_real()),
        });
        // MEAN(array) — arithmetic mean
        self.register("MEAN", 1, |args| match &args[0] {
            Value::Array(ref arr) if !arr.data.is_empty() => {
                let sum: f64 = arr.data.iter().map(|v| v.as_real()).sum();
                Value::Real(sum / arr.data.len() as f64)
            },
            v => Value::Real(v.as_real()),
        });
        // MEDIAN(array) — median value
        self.register("MEDIAN", 1, |args| match &args[0] {
            Value::Array(ref arr) if !arr.data.is_empty() => {
                let mut vals: Vec<f64> = arr.data.iter().map(|v| v.as_real()).collect();
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mid = vals.len() / 2;
                let med = if vals.len() % 2 == 0 {
                    (vals[mid - 1] + vals[mid]) / 2.0
                } else {
                    vals[mid]
                };
                Value::Real(med)
            },
            v => Value::Real(v.as_real()),
        });
        // STD(array) — population standard deviation
        self.register("STD", 1, |args| match &args[0] {
            Value::Array(ref arr) if arr.data.len() > 1 => {
                let n = arr.data.len() as f64;
                let mean: f64 = arr.data.iter().map(|v| v.as_real()).sum::<f64>() / n;
                let variance: f64 = arr
                    .data
                    .iter()
                    .map(|v| {
                        let d = v.as_real() - mean;
                        d * d
                    })
                    .sum::<f64>()
                    / n;
                Value::Real(variance.sqrt())
            },
            _ => Value::Real(0.0),
        });

        // --- FFT stubs ---
        // FFT(real_array, imag_array) — forward FFT
        self.register("FFT", 2, |args| {
            // Stub: return input unchanged
            let re = match &args[0] {
                Value::Array(ref a) => a.clone(),
                _ => ArrayData::new(vec![]),
            };
            Value::Array(re)
        });
        // IFFT(real_array, imag_array) — inverse FFT (stub)
        self.register("IFFT", 2, |args| match &args[0] {
            Value::Array(ref a) => Value::Array(a.clone()),
            _ => Value::Real(0.0),
        });
        // CFFT(complex_array) — complex FFT
        self.register("CFFT", 1, |args| match &args[0] {
            Value::Array(ref arr) => Value::Array(arr.clone()),
            _ => Value::Real(0.0),
        });
        // ICFFT(complex_array) — inverse complex FFT
        self.register("ICFFT", 1, |args| match &args[0] {
            Value::Array(ref arr) => Value::Array(arr.clone()),
            _ => Value::Real(0.0),
        });
    }
}

// ===================== RNG =====================

thread_local! {
    static RNG_SEED: Cell<u64> = Cell::new(12345);
}

/// Fast simple LCG random number in [0, 1).
pub fn rand_fast() -> f64 {
    RNG_SEED.with(|seed| {
        let mut s = seed.get();
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        seed.set(s);
        (s >> 11) as f64 / (1u64 << 53) as f64
    })
}

// ===================== Date/Time Helpers =====================

fn current_date_string() -> String {
    // ISO format: YYYY-MM-DD (HTBasic default)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Rough calculation (ignoring leap years for simplicity)
    let days = secs / 86400;
    let (y, m, d) = days_to_date(days as i64 + 719528); // 719528 = days from 0000-01-01 to 1970-01-01
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn current_time_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = now.as_secs() % 86400;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn current_seconds_since_midnight() -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_secs() % 86400) as f64 + (now.subsec_nanos() as f64 / 1e9)
}

/// Convert days since epoch to (year, month, day).
fn days_to_date(mut days: i64) -> (i64, i64, i64) {
    // Algorithm from Howard Hinnant
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as i64, d as i64)
}
