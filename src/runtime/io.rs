/// I/O system for HTBasic — file handles, device abstraction.
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use crate::runtime::value::Value;

/// An open I/O path.
#[derive(Debug)]
pub struct IoPath {
    pub handle: IoHandle,
    pub is_open: bool,
}

#[derive(Debug)]
pub enum IoHandle {
    File {
        path: String,
        reader: Option<BufReader<File>>,
        writer: Option<File>,
    },
    Device {
        name: String,
    },
    Buffer {
        data: Vec<u8>,
        position: usize,
    },
    /// Standard output (CRT)
    Crt,
    /// Keyboard
    Kbd,
    /// String buffer for OUTPUT KBD
    StringBuffer(String),
}

/// Registry of active I/O paths.
pub struct IoRegistry {
    paths: HashMap<String, IoPath>,
    /// Default mass storage volume
    mass_storage: String,
}

impl IoRegistry {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
            mass_storage: String::new(),
        }
    }

    /// ASSIGN @name TO "filename" [;FORMAT ON/OFF] [;BUFFER n]
    pub fn assign_file(&mut self, name: &str, filename: &str, mode: &str) -> std::io::Result<()> {
        let fname = if !self.mass_storage.is_empty() && !filename.contains(':') {
            format!("{}:{}", self.mass_storage, filename)
        } else {
            filename.to_string()
        };

        // Built-in device names — never open a file for these.
        if fname.eq_ignore_ascii_case("CRT") {
            self.paths.insert(
                name.to_string(),
                IoPath {
                    handle: IoHandle::Crt,
                    is_open: true,
                },
            );
            return Ok(());
        }
        if fname.eq_ignore_ascii_case("KBD") {
            self.paths.insert(
                name.to_string(),
                IoPath {
                    handle: IoHandle::Kbd,
                    is_open: true,
                },
            );
            return Ok(());
        }

        let handle = match mode.to_uppercase().as_str() {
            "OUTPUT" | "WRITE" => {
                let file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&fname)?;
                IoHandle::File {
                    path: fname,
                    reader: None,
                    writer: Some(file),
                }
            },
            "APPEND" => {
                let file = OpenOptions::new().create(true).append(true).open(&fname)?;
                IoHandle::File {
                    path: fname,
                    reader: None,
                    writer: Some(file),
                }
            },
            _ => {
                // Default: read/write
                let read_file = match File::open(&fname) {
                    Ok(f) => Some(BufReader::new(f)),
                    Err(_) => None,
                };
                let write_file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(&fname)
                    .ok();
                IoHandle::File {
                    path: fname,
                    reader: read_file,
                    writer: write_file,
                }
            },
        };

        self.paths.insert(
            name.to_string(),
            IoPath {
                handle,
                is_open: true,
            },
        );
        Ok(())
    }

    /// ASSIGN @name TO DEVICE [address]
    pub fn assign_device(&mut self, name: &str, device: &str, _address: i32) {
        self.paths.insert(
            name.to_string(),
            IoPath {
                handle: IoHandle::Device {
                    name: device.to_string(),
                },
                is_open: true,
            },
        );
    }

    /// ASSIGN @name TO BUFFER [size]
    pub fn assign_buffer(&mut self, name: &str, size: usize) {
        self.paths.insert(
            name.to_string(),
            IoPath {
                handle: IoHandle::Buffer {
                    data: vec![0; size],
                    position: 0,
                },
                is_open: true,
            },
        );
    }

    /// OUTPUT @name; output_data — write data to the path.
    pub fn output(&mut self, name: &str, output_data: &str) -> std::io::Result<()> {
        if let Some(path) = self.paths.get_mut(name) {
            match &mut path.handle {
                IoHandle::File {
                    writer: Some(ref mut w),
                    ..
                } => {
                    w.write_all(output_data.as_bytes())?;
                    w.write_all(b"\n")?;
                    w.flush()?;
                },
                IoHandle::Buffer {
                    ref mut data,
                    ref mut position,
                } => {
                    for b in output_data.as_bytes() {
                        if *position < data.len() {
                            data[*position] = *b;
                            *position += 1;
                        }
                    }
                },
                IoHandle::Crt => {
                    // Stdout — handled by PRINT already
                },
                IoHandle::StringBuffer(ref mut s) => {
                    s.push_str(output_data);
                    s.push('\n');
                },
                _ => {},
            }
        }
        Ok(())
    }

    /// ENTER @name; var — read data from the path.
    pub fn enter(&mut self, name: &str) -> Option<String> {
        if let Some(path) = self.paths.get_mut(name) {
            match &mut path.handle {
                IoHandle::File {
                    reader: Some(ref mut r),
                    ..
                } => {
                    let mut line = String::new();
                    if r.read_line(&mut line).is_ok() {
                        Some(line.trim_end().to_string())
                    } else {
                        None
                    }
                },
                IoHandle::File { .. } => None,
                IoHandle::Buffer {
                    ref data,
                    ref mut position,
                } => {
                    // Read next number from buffer
                    if *position < data.len() {
                        let val = data[*position];
                        *position += 1;
                        Some(val.to_string())
                    } else {
                        None
                    }
                },
                IoHandle::StringBuffer(ref mut s) => {
                    if s.is_empty() {
                        None
                    } else {
                        let result = s.clone();
                        s.clear();
                        Some(result)
                    }
                },
                _ => None,
            }
        } else {
            None
        }
    }

    /// ENTER @name USING format; var — read formatted data.
    pub fn enter_formatted(&mut self, name: &str, _format: &str) -> Option<Value> {
        self.enter(name).map(|s| {
            if let Ok(n) = s.parse::<f64>() {
                Value::Real(n)
            } else {
                Value::string(&s)
            }
        })
    }

    /// Check if a path exists.
    pub fn exists(&self, name: &str) -> bool {
        self.paths.contains_key(name)
    }

    /// True if the path is assigned to the console (CRT).
    pub fn is_crt(&self, name: &str) -> bool {
        self.paths
            .get(name)
            .map(|p| matches!(p.handle, IoHandle::Crt))
            .unwrap_or(false)
    }

    /// Release a path (`ASSIGN @name TO *`).
    pub fn release(&mut self, name: &str) {
        self.paths.remove(name);
    }

    /// Set mass storage volume.
    pub fn mass_storage_is(&mut self, volume: &str) {
        self.mass_storage = volume.to_string();
    }

    /// CREATE file
    pub fn create_file(&self, filename: &str, file_type: &str) -> std::io::Result<()> {
        let fname = self.resolve_filename(filename);
        match file_type.to_uppercase().as_str() {
            "BDAT" => {
                File::create(&fname)?;
            },
            _ => {
                // ASCII — default
                File::create(&fname)?;
            },
        }
        Ok(())
    }

    /// PURGE (delete) a file.
    pub fn purge_file(&self, filename: &str) -> std::io::Result<()> {
        let fname = self.resolve_filename(filename);
        fs::remove_file(&fname)
    }

    /// CAT — list directory.
    pub fn cat(&self, pattern: &str) -> std::io::Result<Vec<String>> {
        let dir = if pattern.is_empty() || pattern == "*" {
            ".".to_string()
        } else {
            self.resolve_filename(pattern)
        };
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    files.push(name.to_string());
                }
            }
        }
        Ok(files)
    }

    fn resolve_filename(&self, filename: &str) -> String {
        if !self.mass_storage.is_empty() && !filename.contains(':') {
            format!("{}:{}", self.mass_storage, filename)
        } else {
            filename.to_string()
        }
    }
}

impl Default for IoRegistry {
    fn default() -> Self {
        Self::new()
    }
}
