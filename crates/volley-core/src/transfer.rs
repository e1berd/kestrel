use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Bytes,
    Requests,
    Files,
    Paths,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub stage: String,
    pub completed: u64,
    pub total: Option<u64>,
    pub unit: Unit,
    pub finished: bool,
    pub cancelled: bool,
    pub output_bytes: u64,
}

impl Snapshot {
    pub fn percent(&self) -> Option<f32> {
        self.total.filter(|total| *total > 0).map(|total| (self.completed as f64 / total as f64 * 100.).min(100.) as f32)
    }
}

#[derive(Debug, Clone)]
pub struct Progress(Arc<Mutex<Snapshot>>);

impl Default for Progress {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Snapshot {
            stage: "Preparing".into(),
            completed: 0,
            total: None,
            unit: Unit::Files,
            finished: false,
            cancelled: false,
            output_bytes: 0,
        })))
    }
}

impl Progress {
    pub fn snapshot(&self) -> Snapshot {
        self.0.lock().expect("transfer progress lock").clone()
    }

    pub fn start(&self, stage: impl Into<String>, total: Option<u64>, unit: Unit) -> Result<()> {
        self.report(stage, total, 0, unit)
    }

    pub fn report(&self, stage: impl Into<String>, total: Option<u64>, completed: u64, unit: Unit) -> Result<()> {
        let mut state = self.0.lock().expect("transfer progress lock");
        if state.cancelled {
            bail!("transfer cancelled");
        }
        state.stage = stage.into();
        state.completed = completed;
        state.total = total;
        state.unit = unit;
        state.finished = false;
        Ok(())
    }

    pub fn advance(&self, amount: u64) -> Result<()> {
        let mut state = self.0.lock().expect("transfer progress lock");
        if state.cancelled {
            bail!("transfer cancelled");
        }
        state.completed += amount;
        Ok(())
    }

    pub fn finish(&self) -> Result<()> {
        self.check()?;
        let mut state = self.0.lock().expect("transfer progress lock");
        if state.total.is_some_and(|total| state.completed < total) {
            bail!("the current transfer stage is incomplete");
        }
        state.finished = true;
        Ok(())
    }

    pub fn cancel(&self) {
        self.0.lock().expect("transfer progress lock").cancelled = true;
    }

    pub fn published(&self, stage: impl Into<String>, total: u64, unit: Unit) {
        let mut state = self.0.lock().expect("transfer progress lock");
        state.stage = stage.into();
        state.total = Some(total);
        state.completed = total;
        state.unit = unit;
        state.finished = true;
    }

    pub fn check(&self) -> Result<()> {
        if self.0.lock().expect("transfer progress lock").cancelled {
            bail!("transfer cancelled");
        }
        Ok(())
    }

    pub fn reader<R: Read>(&self, reader: R) -> ProgressReader<R> {
        ProgressReader { reader, progress: self.clone() }
    }

    pub fn writer<W: Write>(&self, writer: W) -> ProgressWriter<W> {
        ProgressWriter { writer, progress: self.clone() }
    }
}

pub struct ProgressWriter<W> {
    writer: W,
    progress: Progress,
}
impl<W> ProgressWriter<W> {
    pub fn get_ref(&self) -> &W {
        &self.writer
    }
}
impl<W: Write> Write for ProgressWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.progress.check().map_err(io::Error::other)?;
        let count = self.writer.write(bytes)?;
        self.progress.0.lock().expect("transfer progress lock").output_bytes += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub struct ProgressReader<R> {
    reader: R,
    progress: Progress,
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.progress.check().map_err(io::Error::other)?;
        let count = self.reader.read(buffer)?;
        self.progress.advance(count as u64).map_err(io::Error::other)?;
        Ok(count)
    }
}
