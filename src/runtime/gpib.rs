/// GPIB instrument simulator with SCPI command support.
///
/// Instruments respond to IEEE 488.2 common commands (*IDN?, *STB?, *ESR?, *CLS, *RST, *OPC, *WAI)
/// and device-specific SCPI commands.
///
/// GPIB address convention (HP style): 7xx = interface 7, address xx (0-30).
use std::collections::{HashMap, VecDeque};

// ===================== SCPI Error Queue =====================

/// Standard SCPI error queue entry.
#[derive(Debug, Clone)]
pub struct ScpiError {
    pub code: i32,
    pub message: String,
}

/// SCPI-compliant error queue.
#[derive(Debug, Default)]
pub struct ErrorQueue {
    errors: VecDeque<ScpiError>,
}

impl ErrorQueue {
    pub fn new() -> Self {
        let mut q = Self::default();
        q.push(0, "No error");
        q
    }

    pub fn push(&mut self, code: i32, msg: &str) {
        if self.errors.len() > 20 {
            self.errors.pop_front();
        }
        self.errors.push_back(ScpiError {
            code,
            message: msg.to_string(),
        });
    }

    /// Pop oldest error (SYST:ERR?).
    pub fn pop(&mut self) -> ScpiError {
        let err = self.errors.pop_front().unwrap_or(ScpiError {
            code: 0,
            message: "No error".to_string(),
        });
        if self.errors.is_empty() {
            self.errors.push_back(ScpiError {
                code: 0,
                message: "No error".to_string(),
            });
        }
        err
    }

    /// Check if errors are queued (affects ESR).
    pub fn has_error(&self) -> bool {
        self.errors.front().map(|e| e.code != 0).unwrap_or(false)
    }
}

// ===================== SCPI Status System =====================

/// IEEE 488.2 status byte and standard event register.
#[derive(Debug, Default)]
pub struct StatusSystem {
    /// Status Byte Register (read by *STB?, serial poll)
    pub stb: u8,
    /// Service Request Enable mask
    pub sre: u8,
    /// Standard Event Status Register
    pub esr: u8,
    /// Standard Event Status Enable mask
    pub ese: u8,
    /// Operation complete flag
    pub opc: bool,
}

impl StatusSystem {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set bit in ESR (used by error queue, OPC, etc.)
    pub fn set_esr(&mut self, bit: u8) {
        self.esr |= 1 << bit;
        if self.esr & self.ese != 0 {
            self.stb |= 0x20; // ESB (Event Summary Bit)
        }
    }

    /// Clear ESR and STB
    pub fn clear(&mut self) {
        self.esr = 0;
        self.stb &= !0x20;
    }

    /// Message Available (MAV) bit — set when response data is ready
    pub fn set_mav(&mut self) {
        self.stb |= 0x10;
    }

    pub fn clear_mav(&mut self) {
        self.stb &= !0x10;
    }
}

// ===================== Virtual Instrument Trait =====================

pub trait Instrument: Send {
    fn command(&mut self, cmd: &str);
    fn query(&mut self, cmd: &str) -> String;
    fn status_byte(&self) -> u8;
    fn set_status_byte(&mut self, byte: u8);
    fn error_queue(&mut self) -> &mut ErrorQueue;
    fn status_system(&mut self) -> &mut StatusSystem;

    /// Handle IEEE 488.2 common commands (*IDN?, *STB?, etc.)
    fn handle_common(&mut self, cmd: &str) -> Option<String> {
        let upper = cmd.trim().to_uppercase();
        match upper.as_str() {
            "*IDN?" => Some("HTBasic,GPIB Simulator,0,1.0".to_string()),
            "*STB?" => {
                let stb = self.status_byte();
                Some(stb.to_string())
            },
            "*ESR?" => {
                let esr = self.status_system().esr;
                self.status_system().esr = 0; // ESR clears on read
                Some(esr.to_string())
            },
            "*ESE?" => Some(self.status_system().ese.to_string()),
            "*ESE" => {
                // *ESE <value> should be handled by command(), this is query only
                Some(self.status_system().ese.to_string())
            },
            "*SRE?" => Some(self.status_system().sre.to_string()),
            "*CLS" => {
                self.status_system().clear();
                self.error_queue().push(0, "No error");
                None
            },
            "*RST" => {
                self.status_system().clear();
                self.error_queue().push(0, "No error");
                None
            },
            "*OPC" => {
                self.status_system().opc = true;
                self.status_system().set_esr(0);
                None
            },
            "*OPC?" => {
                self.status_system().opc = true;
                self.status_system().set_esr(0);
                Some("1".to_string())
            },
            "*WAI" => None, // No-op in simulation
            "SYST:ERR?" | "SYSTEM:ERROR?" => {
                let err = self.error_queue().pop();
                Some(format!("{},\"{}\"", err.code, err.message))
            },
            _ => None,
        }
    }

    /// Handle SCPI status commands.
    fn handle_status(&mut self, cmd: &str) -> Option<String> {
        let upper = cmd.trim().to_uppercase();
        match upper.as_str() {
            "STAT:OPER?" | "STAT:OPER:EVEN?" => Some("0".to_string()),
            "STAT:OPER:COND?" => Some("0".to_string()),
            "STAT:OPER:ENAB?" => Some("0".to_string()),
            "STAT:QUES?" | "STAT:QUES:EVEN?" => Some("0".to_string()),
            "STAT:QUES:COND?" => Some("0".to_string()),
            "STAT:PRES" => None,
            _ => None,
        }
    }
}

// ===================== DMM (HP 34401A) =====================

#[derive(Clone, Copy, PartialEq)]
pub enum DmmFunction {
    DcVoltage,
    AcVoltage,
    DcCurrent,
    AcCurrent,
    Resistance2W,
    Resistance4W,
    Frequency,
    Period,
    Continuity,
    Diode,
}

pub struct Dmm {
    function: DmmFunction,
    range: f64,
    resolution: f64,
    nplc: f64, // integration time in power line cycles
    auto_zero: bool,
    counter: usize,
    error_queue: ErrorQueue,
    status: StatusSystem,
    last_reading: f64,
}

impl Dmm {
    pub fn new() -> Self {
        Self {
            function: DmmFunction::DcVoltage,
            range: 10.0,          // 10V range
            resolution: 0.000001, // 6.5 digits
            nplc: 10.0,
            auto_zero: true,
            counter: 0,
            error_queue: ErrorQueue::new(),
            status: StatusSystem::new(),
            last_reading: 0.0,
        }
    }

    fn take_reading(&mut self) -> f64 {
        self.counter += 1;
        // Generate a realistic-looking measurement with noise
        let base = match self.function {
            DmmFunction::DcVoltage => 5.0 * (self.counter as f64 * 0.137).sin() + 2.5,
            DmmFunction::AcVoltage => 3.536 * (self.counter as f64 * 0.2).sin().abs(),
            DmmFunction::DcCurrent => ((self.counter % 10) as f64) * 0.01 + 0.001,
            DmmFunction::AcCurrent => (self.counter as f64 * 0.3).sin().abs() * 0.01,
            DmmFunction::Resistance2W | DmmFunction::Resistance4W => {
                1000.0 + ((self.counter % 20) as f64) * 47.0
            },
            DmmFunction::Frequency => 1000.0 + ((self.counter % 5) as f64) * 0.1,
            DmmFunction::Period => 1.0 / (1000.0 + self.counter as f64 * 0.01),
            DmmFunction::Continuity => {
                if self.counter % 3 == 0 {
                    0.1
                } else {
                    1e9
                }
            },
            DmmFunction::Diode => 0.6 + ((self.counter % 10) as f64) * 0.01,
        };
        // Add noise (10 ppm of range + 1 ppm of reading)
        let noise = (self.range * 10e-6 + base * 1e-6) * (self.counter as f64 * 7919.0).sin();
        let reading = base + noise;
        self.last_reading = reading;
        self.status.set_mav();
        reading
    }
}

impl Instrument for Dmm {
    fn command(&mut self, cmd: &str) {
        let upper = cmd.trim().to_uppercase();
        // Handle *ESE <value>, *SRE <value> as commands (not queries)
        if upper.starts_with("*ESE ") {
            if let Ok(v) = upper[5..].trim().parse::<u8>() {
                self.status.ese = v;
            }
            return;
        }
        if upper.starts_with("*SRE ") {
            if let Ok(v) = upper[5..].trim().parse::<u8>() {
                self.status.sre = v;
            }
            return;
        }
        if upper.starts_with("CONF:") || upper.starts_with("CONFIGURE:") {
            if upper.contains("VOLT:DC") {
                self.function = DmmFunction::DcVoltage;
            } else if upper.contains("VOLT:AC") {
                self.function = DmmFunction::AcVoltage;
            } else if upper.contains("CURR:DC") {
                self.function = DmmFunction::DcCurrent;
            } else if upper.contains("CURR:AC") {
                self.function = DmmFunction::AcCurrent;
            } else if upper.contains("RES") {
                self.function = DmmFunction::Resistance2W;
            } else if upper.contains("FRES") {
                self.function = DmmFunction::Resistance4W;
            } else if upper.contains("FREQ") {
                self.function = DmmFunction::Frequency;
            } else if upper.contains("PER") {
                self.function = DmmFunction::Period;
            } else if upper.contains("CONT") {
                self.function = DmmFunction::Continuity;
            } else if upper.contains("DIOD") {
                self.function = DmmFunction::Diode;
            }
            return;
        }
        if upper.starts_with("SENS:") || upper.starts_with("SENSE:") || upper.starts_with("MEAS:") {
            if upper.contains("VOLT:DC") {
                self.function = DmmFunction::DcVoltage;
            } else if upper.contains("VOLT:AC") {
                self.function = DmmFunction::AcVoltage;
            } else if upper.contains("CURR:DC") {
                self.function = DmmFunction::DcCurrent;
            } else if upper.contains("CURR:AC") {
                self.function = DmmFunction::AcCurrent;
            } else if upper.contains("RES") {
                self.function = DmmFunction::Resistance2W;
            } else if upper.contains("FRES") {
                self.function = DmmFunction::Resistance4W;
            } else if upper.contains("FREQ") {
                self.function = DmmFunction::Frequency;
            }
            // If it's a query (ends with ?), handle in query()
            return;
        }
        // Range and resolution
        if upper.starts_with("RANGE ") || upper.starts_with("SENS:VOLT:DC:RANG ") {
            if let Ok(r) = upper
                .split_whitespace()
                .last()
                .unwrap_or("10")
                .parse::<f64>()
            {
                self.range = r;
            }
        }
        if upper.starts_with("RES ") || upper.starts_with("SENS:VOLT:DC:RES ") {
            if let Ok(r) = upper
                .split_whitespace()
                .last()
                .unwrap_or("0.000001")
                .parse::<f64>()
            {
                self.resolution = r;
            }
        }
        if upper.starts_with("NPLC ") || upper.starts_with("SENS:VOLT:DC:NPLC ") {
            if let Ok(n) = upper
                .split_whitespace()
                .last()
                .unwrap_or("10")
                .parse::<f64>()
            {
                self.nplc = n;
            }
        }
        if upper == "AUTO OFF" || upper == "SENS:VOLT:DC:ZERO:AUTO OFF" {
            self.auto_zero = false;
        }
        if upper == "AUTO ON" || upper == "SENS:VOLT:DC:ZERO:AUTO ON" {
            self.auto_zero = true;
        }
    }

    fn query(&mut self, cmd: &str) -> String {
        let upper = cmd.trim().to_uppercase();
        // IEEE 488.2 common commands
        if let Some(resp) = self.handle_common(cmd) {
            return resp;
        }
        // SCPI status
        if let Some(resp) = self.handle_status(cmd) {
            return resp;
        }
        // Configuration queries
        if upper.starts_with("CONF?") {
            let func = match self.function {
                DmmFunction::DcVoltage => "VOLT:DC",
                DmmFunction::AcVoltage => "VOLT:AC",
                DmmFunction::DcCurrent => "CURR:DC",
                DmmFunction::AcCurrent => "CURR:AC",
                DmmFunction::Resistance2W => "RES",
                DmmFunction::Resistance4W => "FRES",
                DmmFunction::Frequency => "FREQ",
                DmmFunction::Period => "PER",
                DmmFunction::Continuity => "CONT",
                DmmFunction::Diode => "DIOD",
            };
            return format!("{} {:.3},{}", func, self.range, self.resolution);
        }
        // Measurement queries
        if upper == "READ?"
            || upper.ends_with(":DC?")
            || upper.ends_with(":AC?")
            || upper.ends_with("RES?")
            || upper.ends_with(":FREQ?")
            || upper.ends_with(":PER?")
            || upper.starts_with("MEAS:")
            || upper == "INIT"
            || upper == "INIT;*WAI;FETCH?"
        {
            let reading = self.take_reading();
            return format!("{:.9}", reading);
        }
        // Configuration queries
        if upper == "RANGE?" {
            return format!("{:.3}", self.range);
        }
        if upper == "RES?" {
            return format!("{:.9}", self.resolution);
        }
        if upper == "NPLC?" {
            return format!("{:.1}", self.nplc);
        }
        // Default — treat as measurement
        if upper.ends_with('?') {
            return format!("{:.9}", self.take_reading());
        }
        String::new()
    }

    fn status_byte(&self) -> u8 {
        self.status.stb
    }
    fn set_status_byte(&mut self, byte: u8) {
        self.status.stb = byte;
    }
    fn error_queue(&mut self) -> &mut ErrorQueue {
        &mut self.error_queue
    }
    fn status_system(&mut self) -> &mut StatusSystem {
        &mut self.status
    }
}

// ===================== Function Generator (HP 33120A) =====================

pub struct FuncGen {
    frequency: f64,
    amplitude: f64,
    offset: f64,
    waveform: String,
    output_enabled: bool,
    burst_count: u32,
    burst_enabled: bool,
    error_queue: ErrorQueue,
    status: StatusSystem,
}

impl FuncGen {
    pub fn new() -> Self {
        Self {
            frequency: 1000.0,
            amplitude: 1.0,
            offset: 0.0,
            waveform: "SIN".to_string(),
            output_enabled: false,
            burst_count: 1,
            burst_enabled: false,
            error_queue: ErrorQueue::new(),
            status: StatusSystem::new(),
        }
    }
}

impl Instrument for FuncGen {
    fn command(&mut self, cmd: &str) {
        let upper = cmd.trim().to_uppercase();
        if upper.starts_with("*ESE ") {
            if let Ok(v) = upper[5..].trim().parse::<u8>() {
                self.status.ese = v;
            }
            return;
        }
        if upper.starts_with("*SRE ") {
            if let Ok(v) = upper[5..].trim().parse::<u8>() {
                self.status.sre = v;
            }
            return;
        }
        if upper.starts_with("FREQ ") {
            if let Ok(f) = upper[5..].trim().parse::<f64>() {
                if f > 0.0 {
                    self.frequency = f;
                } else {
                    self.error_queue.push(-222, "Frequency must be positive");
                }
            }
        } else if upper.starts_with("VOLT ") {
            if let Ok(v) = upper[5..].trim().parse::<f64>() {
                self.amplitude = v;
            }
        } else if upper.starts_with("VOLT:OFFS ") {
            if let Ok(o) = upper[10..].trim().parse::<f64>() {
                self.offset = o;
            }
        } else if upper.starts_with("FUNC ") || upper.starts_with("WAVE ") {
            let wf = upper[5..].trim().to_uppercase();
            if matches!(
                wf.as_str(),
                "SIN" | "SQU" | "TRI" | "RAMP" | "NOIS" | "DC" | "USER"
            ) {
                self.waveform = wf;
            } else {
                self.error_queue.push(-224, "Illegal waveform parameter");
            }
        } else if upper == "OUTP ON" || upper == "OUTPUT ON" || upper == "OUTP 1" {
            self.output_enabled = true;
        } else if upper == "OUTP OFF" || upper == "OUTPUT OFF" || upper == "OUTP 0" {
            self.output_enabled = false;
        } else if upper.starts_with("BURS:NCYC ") {
            if let Ok(n) = upper[10..].trim().parse::<u32>() {
                self.burst_count = n;
            }
        } else if upper == "BURS ON" || upper == "BURST ON" {
            self.burst_enabled = true;
        } else if upper == "BURS OFF" || upper == "BURST OFF" {
            self.burst_enabled = false;
        }
    }

    fn query(&mut self, cmd: &str) -> String {
        let upper = cmd.trim().to_uppercase();
        if let Some(resp) = self.handle_common(cmd) {
            return resp;
        }
        if let Some(resp) = self.handle_status(cmd) {
            return resp;
        }
        match upper.as_str() {
            "FREQ?" => format!("{:.9}", self.frequency),
            "VOLT?" | "AMPL?" => format!("{:.6}", self.amplitude),
            "VOLT:OFFS?" => format!("{:.6}", self.offset),
            "FUNC?" | "WAVE?" => self.waveform.clone(),
            "OUTP?" => if self.output_enabled { "1" } else { "0" }.to_string(),
            "BURS:NCYC?" => self.burst_count.to_string(),
            "BURS?" => if self.burst_enabled { "1" } else { "0" }.to_string(),
            _ => String::new(),
        }
    }

    fn status_byte(&self) -> u8 {
        self.status.stb
    }
    fn set_status_byte(&mut self, byte: u8) {
        self.status.stb = byte;
    }
    fn error_queue(&mut self) -> &mut ErrorQueue {
        &mut self.error_queue
    }
    fn status_system(&mut self) -> &mut StatusSystem {
        &mut self.status
    }
}

// ===================== Oscilloscope (HP 54600A) =====================

pub struct Scope {
    timebase: f64,
    volts_per_div: f64,
    trigger_level: f64,
    trigger_source: String,
    trigger_mode: String,
    channel_count: u8,
    counter: usize,
    error_queue: ErrorQueue,
    status: StatusSystem,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            timebase: 1.0e-3,
            volts_per_div: 1.0,
            trigger_level: 0.0,
            trigger_source: "CH1".to_string(),
            trigger_mode: "AUTO".to_string(),
            channel_count: 2,
            counter: 0,
            error_queue: ErrorQueue::new(),
            status: StatusSystem::new(),
        }
    }
}

impl Instrument for Scope {
    fn command(&mut self, cmd: &str) {
        let upper = cmd.trim().to_uppercase();
        if upper.starts_with("*ESE ") {
            if let Ok(v) = upper[5..].trim().parse::<u8>() {
                self.status.ese = v;
            }
            return;
        }
        if upper.starts_with("TIM ")
            || upper.starts_with("TIMEBASE ")
            || upper.starts_with("TIM:SCAL ")
        {
            if let Ok(t) = upper
                .split_whitespace()
                .last()
                .unwrap_or("0.001")
                .parse::<f64>()
            {
                self.timebase = t;
            }
        } else if upper.starts_with("CHAN1:SCAL ")
            || upper.starts_with("CH1:SCAL ")
            || upper.starts_with("VOLT ")
        {
            if let Ok(v) = upper
                .split_whitespace()
                .last()
                .unwrap_or("1.0")
                .parse::<f64>()
            {
                self.volts_per_div = v;
            }
        } else if upper.starts_with("TRIG:LEV ") || upper.starts_with("TRIGGER:LEVEL ") {
            if let Ok(l) = upper
                .split_whitespace()
                .last()
                .unwrap_or("0.0")
                .parse::<f64>()
            {
                self.trigger_level = l;
            }
        } else if upper.starts_with("TRIG:SOUR ") {
            self.trigger_source = upper.split_whitespace().last().unwrap_or("CH1").to_string();
        } else if upper == "RUN" || upper == "SINGLE" || upper == "ACQUIRE:STATE ON" {
            self.counter = 0;
        } else if upper == "STOP" || upper == "ACQUIRE:STATE OFF" {
            // Stop acquisition
        }
    }

    fn query(&mut self, cmd: &str) -> String {
        let upper = cmd.trim().to_uppercase();
        if let Some(resp) = self.handle_common(cmd) {
            return resp;
        }
        if let Some(resp) = self.handle_status(cmd) {
            return resp;
        }
        self.counter += 1;
        match upper.as_str() {
            "TIM?" | "TIM:SCAL?" | "TIMEBASE?" => format!("{:.9}", self.timebase),
            "CHAN1:SCAL?" | "CH1:SCAL?" | "VDIV?" => format!("{:.6}", self.volts_per_div),
            "TRIG:LEV?" => format!("{:.6}", self.trigger_level),
            "TRIG:SOUR?" => self.trigger_source.clone(),
            "TRIG:MODE?" => self.trigger_mode.clone(),
            "WAV:PRE?" => format!("{},1000,{}", self.timebase, self.volts_per_div),
            "WAV:DATA?" => {
                let val = (self.counter as f64 * 0.1).sin() * self.volts_per_div;
                format!("{:.9}", val)
            },
            "CHAN1:DATA?" => {
                let val = (self.counter as f64 * 0.2).cos() * self.volts_per_div;
                format!("{:.9}", val)
            },
            _ => String::new(),
        }
    }

    fn status_byte(&self) -> u8 {
        self.status.stb
    }
    fn set_status_byte(&mut self, byte: u8) {
        self.status.stb = byte;
    }
    fn error_queue(&mut self) -> &mut ErrorQueue {
        &mut self.error_queue
    }
    fn status_system(&mut self) -> &mut StatusSystem {
        &mut self.status
    }
}

// ===================== GPIB Bus =====================

pub struct GpibBus {
    devices: HashMap<u8, Box<dyn Instrument>>,
}

impl GpibBus {
    pub fn new() -> Self {
        let mut bus = Self {
            devices: HashMap::new(),
        };
        // Pre-register default instruments
        bus.add_device(22, Box::new(Dmm::new()));
        bus.add_device(10, Box::new(FuncGen::new()));
        bus.add_device(7, Box::new(Scope::new()));
        bus
    }

    pub fn add_device(&mut self, address: u8, device: Box<dyn Instrument>) {
        self.devices.insert(address, device);
    }

    pub fn has_device(&self, address: u8) -> bool {
        self.devices.contains_key(&address)
    }

    pub fn output(&mut self, address: u8, data: &str) -> String {
        if let Some(dev) = self.devices.get_mut(&address) {
            let trimmed = data.trim();
            if trimmed.ends_with('?') {
                dev.query(trimmed)
            } else {
                dev.command(trimmed);
                String::new()
            }
        } else {
            String::new()
        }
    }

    pub fn enter(&mut self, address: u8, query: &str) -> String {
        if let Some(dev) = self.devices.get_mut(&address) {
            dev.query(query.trim())
        } else {
            "0.0".to_string()
        }
    }

    pub fn status(&self, address: u8, _register: u8) -> u8 {
        if let Some(dev) = self.devices.get(&address) {
            dev.status_byte()
        } else {
            0
        }
    }

    pub fn control(&mut self, address: u8, _register: u8, value: u8) {
        if let Some(dev) = self.devices.get_mut(&address) {
            dev.set_status_byte(value);
        }
    }

    pub fn list_devices(&self) -> Vec<(u8, String)> {
        self.devices
            .iter()
            .map(|(&a, _)| (a, format!("Device at {}", a)))
            .collect()
    }
}

impl Default for GpibBus {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_gpib_address(addr_str: &str) -> Option<u8> {
    if let Ok(addr) = addr_str.parse::<u32>() {
        if (700..800).contains(&addr) {
            Some((addr % 100) as u8)
        } else if addr < 31 {
            Some(addr as u8)
        } else {
            None
        }
    } else {
        None
    }
}
