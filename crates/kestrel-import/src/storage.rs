use std::fs::File;
use std::io::{BufReader, BufWriter, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use anyhow::Result;
use kestrel_core::report::ReportExt as _;
use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug)]
pub(crate) struct TemporaryDirectory(pub PathBuf);

impl TemporaryDirectory {
    pub fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("kestrel-transfer-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).report_err("remove transfer working files");
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Location {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug)]
pub(crate) struct Store {
    writer: BufWriter<File>,
    reader: File,
    position: u64,
}

impl Store {
    pub fn len(&self) -> u64 {
        self.position
    }
    pub fn new(path: &Path) -> Result<Self> {
        Ok(Self { writer: BufWriter::new(File::create(path)?), reader: File::open(path)?, position: 0 })
    }

    pub fn append(&mut self, value: &impl Serialize) -> Result<Location> {
        let offset = self.position;
        let mut counter = CountWriter { writer: &mut self.writer, count: 0 };
        serde_json::to_writer(&mut counter, value)?;
        let length = counter.count;
        self.position += length;
        Ok(Location { offset, length })
    }

    pub fn read<T: DeserializeOwned>(&mut self, location: Location) -> Result<T> {
        self.writer.flush()?;
        self.reader.seek(SeekFrom::Start(location.offset))?;
        Ok(serde_json::from_reader(BufReader::new((&mut self.reader).take(location.length)))?)
    }
}

struct CountWriter<'a, W> {
    writer: &'a mut W,
    count: u64,
}

impl<W: std::io::Write> std::io::Write for CountWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.writer.write(bytes)?;
        self.count += count as u64;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
