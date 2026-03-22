//! Solution path tracing for offline analysis of DMM dynamics.
//!
//! Compile-time gated: only included with `cargo build --features trace`.
//! Records discrete Boolean trajectory (variable flips) and optionally
//! memory variable snapshots for investigating oscillation patterns,
//! stagnation, and variable behavior.

use std::fs::File;
use std::io::{self, BufWriter, Write};

/// Magic bytes for trace file header.
const MAGIC: &[u8; 4] = b"SPTR";
/// File format version.
const VERSION: u8 = 1;

/// Restart sentinel: time=NaN, var=u32::MAX, val=0xFF.
const RESTART_MARKER_VAR: u32 = u32::MAX;
const RESTART_MARKER_VAL: u8 = 0xFF;

#[derive(Clone, Debug)]
pub enum TraceMode {
    /// Record every individual variable sign flip.
    Full,
    /// Record full Boolean assignment every N steps.
    Snapshot { interval_steps: u64 },
}

#[derive(Clone, Debug)]
pub struct TraceConfig {
    pub mode: TraceMode,
    pub output_path: String,
    pub trace_memory: bool,
}

/// Collects trace events and writes them to a binary file.
pub struct TraceCollector {
    writer: BufWriter<File>,
    prev_signs: Vec<u8>,
    step_count: u64,
    num_vars: usize,
    num_clauses: usize,
    mode: TraceMode,
    trace_memory: bool,
    initialized: bool,
}

impl TraceCollector {
    /// Create a new trace collector and write the file header.
    pub fn new(
        config: &TraceConfig,
        num_vars: usize,
        num_clauses: usize,
    ) -> io::Result<Self> {
        let file = File::create(&config.output_path)?;
        let mut writer = BufWriter::with_capacity(64 * 1024, file);

        // Write header
        writer.write_all(MAGIC)?;
        writer.write_all(&[VERSION])?;
        let mode_byte = match config.mode {
            TraceMode::Full => 0u8,
            TraceMode::Snapshot { .. } => 1u8,
        };
        writer.write_all(&[mode_byte])?;
        writer.write_all(&(num_vars as u32).to_le_bytes())?;
        writer.write_all(&(num_clauses as u32).to_le_bytes())?;
        let flags = if config.trace_memory { 1u8 } else { 0u8 };
        writer.write_all(&[flags])?;

        // For snapshot mode, write the interval
        if let TraceMode::Snapshot { interval_steps } = config.mode {
            writer.write_all(&interval_steps.to_le_bytes())?;
        }

        Ok(TraceCollector {
            writer,
            prev_signs: vec![0u8; num_vars],
            step_count: 0,
            num_vars,
            num_clauses,
            mode: config.mode.clone(),
            trace_memory: config.trace_memory,
            initialized: false,
        })
    }

    /// Initialize prev_signs from the initial voltage state.
    /// Must be called before the first integration step.
    pub fn init_signs(&mut self, v: &[f64]) {
        for (i, &vi) in v.iter().enumerate() {
            self.prev_signs[i] = if vi >= 0.0 { 1 } else { 0 };
        }
        self.initialized = true;
    }

    /// Record one integration step. Call after each euler_step.
    pub fn record_step(&mut self, t: f64, v: &[f64]) {
        if !self.initialized {
            self.init_signs(v);
            self.step_count += 1;
            return;
        }

        match self.mode {
            TraceMode::Full => self.record_flips(t, v),
            TraceMode::Snapshot { interval_steps } => {
                if self.step_count % interval_steps == 0 {
                    self.record_snapshot(t, v);
                }
            }
        }

        // Update prev_signs
        for (i, &vi) in v.iter().enumerate() {
            self.prev_signs[i] = if vi >= 0.0 { 1 } else { 0 };
        }

        self.step_count += 1;
    }

    /// Record a restart marker so analysis can segment by restart.
    pub fn record_restart(&mut self, t: f64, v: &[f64]) {
        // Emit sentinel record
        let _ = self.writer.write_all(&f64::NAN.to_le_bytes());
        let _ = self.writer.write_all(&RESTART_MARKER_VAR.to_le_bytes());
        let _ = self.writer.write_all(&[RESTART_MARKER_VAL]);

        // Re-initialize signs from new state
        self.init_signs(v);
        let _ = self.writer.flush();
    }

    /// Optionally record memory variable snapshots.
    pub fn record_memory_step(&mut self, t: f64, x_s: &[f64], x_l: &[f64]) {
        if !self.trace_memory {
            return;
        }
        let _ = self.writer.write_all(&t.to_le_bytes());
        for &xs in x_s {
            let _ = self.writer.write_all(&xs.to_le_bytes());
        }
        for &xl in x_l {
            let _ = self.writer.write_all(&xl.to_le_bytes());
        }
    }

    /// Flush and finalize the trace file.
    pub fn finish(mut self) -> io::Result<()> {
        self.writer.flush()
    }

    // --- Private methods ---

    fn record_flips(&mut self, t: f64, v: &[f64]) {
        for i in 0..self.num_vars {
            let new_sign = if v[i] >= 0.0 { 1u8 } else { 0u8 };
            if new_sign != self.prev_signs[i] {
                // Emit flip event: f64 time + u32 var + u8 val = 13 bytes
                let _ = self.writer.write_all(&t.to_le_bytes());
                let _ = self.writer.write_all(&(i as u32).to_le_bytes());
                let _ = self.writer.write_all(&[new_sign]);
            }
        }
    }

    fn record_snapshot(&mut self, t: f64, v: &[f64]) {
        let _ = self.writer.write_all(&t.to_le_bytes());
        // Pack booleans: ceil(num_vars / 8) bytes
        let num_bytes = (self.num_vars + 7) / 8;
        let mut packed = vec![0u8; num_bytes];
        for i in 0..self.num_vars {
            if v[i] >= 0.0 {
                packed[i / 8] |= 1 << (i % 8);
            }
        }
        let _ = self.writer.write_all(&packed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_trace_header_full_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let config = TraceConfig {
            mode: TraceMode::Full,
            output_path: path.to_str().unwrap().to_string(),
            trace_memory: false,
        };
        let tc = TraceCollector::new(&config, 10, 20).unwrap();
        tc.finish().unwrap();

        let mut data = Vec::new();
        File::open(&path).unwrap().read_to_end(&mut data).unwrap();

        assert_eq!(&data[0..4], b"SPTR");
        assert_eq!(data[4], 1); // version
        assert_eq!(data[5], 0); // mode = Full
        assert_eq!(u32::from_le_bytes([data[6], data[7], data[8], data[9]]), 10); // num_vars
        assert_eq!(u32::from_le_bytes([data[10], data[11], data[12], data[13]]), 20); // num_clauses
        assert_eq!(data[14], 0); // flags: no trace_memory
    }

    #[test]
    fn test_trace_records_flips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let config = TraceConfig {
            mode: TraceMode::Full,
            output_path: path.to_str().unwrap().to_string(),
            trace_memory: false,
        };
        let mut tc = TraceCollector::new(&config, 3, 5).unwrap();

        // Initial state: [0.5, -0.3, 0.1] → signs [1, 0, 1]
        tc.record_step(0.0, &[0.5, -0.3, 0.1]);
        // Step: [0.5, 0.3, -0.1] → var 1 flips to 1, var 2 flips to 0
        tc.record_step(1.0, &[0.5, 0.3, -0.1]);
        tc.finish().unwrap();

        let mut data = Vec::new();
        File::open(&path).unwrap().read_to_end(&mut data).unwrap();

        // Header is 15 bytes, then 2 flip records of 13 bytes each
        let body = &data[15..];
        assert_eq!(body.len(), 26); // 2 * 13

        // First flip: time=1.0, var=1, val=1
        let t = f64::from_le_bytes(body[0..8].try_into().unwrap());
        let var = u32::from_le_bytes(body[8..12].try_into().unwrap());
        let val = body[12];
        assert_eq!(t, 1.0);
        assert_eq!(var, 1);
        assert_eq!(val, 1);

        // Second flip: time=1.0, var=2, val=0
        let t2 = f64::from_le_bytes(body[13..21].try_into().unwrap());
        let var2 = u32::from_le_bytes(body[21..25].try_into().unwrap());
        let val2 = body[25];
        assert_eq!(t2, 1.0);
        assert_eq!(var2, 2);
        assert_eq!(val2, 0);
    }

    #[test]
    fn test_snapshot_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let config = TraceConfig {
            mode: TraceMode::Snapshot { interval_steps: 2 },
            output_path: path.to_str().unwrap().to_string(),
            trace_memory: false,
        };
        let mut tc = TraceCollector::new(&config, 10, 20).unwrap();

        // Step 0 initializes, step 1 is first real step
        tc.record_step(0.0, &[0.5; 10]);
        // step_count=1, not multiple of 2 → no snapshot
        tc.record_step(1.0, &[0.5; 10]);
        // step_count=2, multiple of 2 → snapshot
        tc.record_step(2.0, &[0.5; 10]);
        // step_count=3, no snapshot
        tc.record_step(3.0, &[-0.5; 10]);
        // step_count=4 → snapshot
        tc.record_step(4.0, &[-0.5; 10]);

        tc.finish().unwrap();

        let mut data = Vec::new();
        File::open(&path).unwrap().read_to_end(&mut data).unwrap();

        // Header: 15 + 8 (interval) = 23 bytes
        // Each snapshot: 8 (time) + ceil(10/8)=2 bytes = 10 bytes
        // 2 snapshots = 20 bytes
        let body = &data[23..];
        assert_eq!(body.len(), 20);
    }

    #[test]
    fn test_restart_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.bin");
        let config = TraceConfig {
            mode: TraceMode::Full,
            output_path: path.to_str().unwrap().to_string(),
            trace_memory: false,
        };
        let mut tc = TraceCollector::new(&config, 3, 5).unwrap();
        tc.init_signs(&[0.5, -0.3, 0.1]);
        tc.record_restart(10.0, &[-0.5, 0.3, -0.1]);
        tc.finish().unwrap();

        let mut data = Vec::new();
        File::open(&path).unwrap().read_to_end(&mut data).unwrap();

        let body = &data[15..];
        assert_eq!(body.len(), 13);
        let var = u32::from_le_bytes(body[8..12].try_into().unwrap());
        let val = body[12];
        assert_eq!(var, RESTART_MARKER_VAR);
        assert_eq!(val, RESTART_MARKER_VAL);
    }
}
