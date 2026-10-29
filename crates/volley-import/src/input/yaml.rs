use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufWriter, Read, Seek as _, SeekFrom, Write};
use std::path::Path;
use std::rc::Rc;

use anyhow::{Result, bail};
use serde_json::Value as Json;
use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::scanner::TScalarStyle;

struct Characters<R> {
    reader: R,
    error: Rc<RefCell<Option<io::Error>>>,
}

impl<R: Read> Iterator for Characters<R> {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        let result = (|| -> io::Result<Option<char>> {
            let mut bytes = [0; 4];
            if self.reader.read(&mut bytes[..1])? == 0 {
                return Ok(None);
            }
            let length = match bytes[0] {
                0..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "input is not UTF-8")),
            };
            self.reader.read_exact(&mut bytes[1..length])?;
            Ok(std::str::from_utf8(&bytes[..length]).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?.chars().next())
        })();
        match result {
            Ok(character) => character,
            Err(error) => {
                *self.error.borrow_mut() = Some(error);
                None
            }
        }
    }
}

struct Frame {
    map: bool,
    key: bool,
    count: usize,
    anchor: usize,
    start: u64,
}

struct Output {
    writer: BufWriter<File>,
    position: u64,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = self.writer.write(bytes)?;
        self.position += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub(super) fn translate(reader: impl Read, path: &Path) -> Result<()> {
    let error = Rc::new(RefCell::new(None));
    let mut parser = Parser::new(Characters { reader, error: error.clone() });
    let mut output = Output { writer: BufWriter::new(File::create(path)?), position: 0 };
    let mut alias_reader = File::open(path)?;
    let mut frames: Vec<Frame> = Vec::new();
    let mut anchors = HashMap::new();
    let mut documents = 0;
    let result = (|| -> Result<()> {
        loop {
            let (event, _) = parser.next_token()?;
            match event {
                Event::StreamEnd => break,
                Event::DocumentStart => {
                    documents += 1;
                    if documents > 1 {
                        bail!("import one YAML document at a time");
                    }
                }
                Event::Scalar(text, style, anchor, tag) => {
                    let key = frames.last().is_some_and(|frame| frame.map && frame.key);
                    if key && text == "<<" && style == TScalarStyle::Plain {
                        bail!("YAML merge keys are not supported; expand the merged mapping before importing");
                    }
                    before(&mut frames, &mut output)?;
                    let start = output.position;
                    let value = if key || style != TScalarStyle::Plain || tag.as_ref().is_some_and(|tag| tag.suffix == "str") {
                        Json::String(text)
                    } else {
                        let scalar: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
                        serde_json::to_value(scalar)?
                    };
                    serde_json::to_writer(&mut output, &value)?;
                    if anchor > 0 {
                        anchors.insert(anchor, (start, output.position - start));
                    }
                    if key {
                        output.write_all(b":")?;
                        frames.last_mut().expect("a YAML mapping").key = false;
                    } else {
                        after(&mut frames);
                    }
                }
                Event::MappingStart(anchor, _) | Event::SequenceStart(anchor, _) => {
                    if frames.last().is_some_and(|frame| frame.map && frame.key) {
                        bail!("YAML mapping keys must be scalars");
                    }
                    if frames.len() > 128 {
                        bail!("YAML is nested more than 128 levels deep");
                    }
                    before(&mut frames, &mut output)?;
                    let map = matches!(event, Event::MappingStart(..));
                    let start = output.position;
                    output.write_all(if map { b"{" } else { b"[" })?;
                    frames.push(Frame { map, key: map, count: 0, anchor, start });
                }
                Event::MappingEnd | Event::SequenceEnd => {
                    let frame = frames.pop().expect("an open YAML container");
                    output.write_all(if frame.map { b"}" } else { b"]" })?;
                    if frame.anchor > 0 {
                        anchors.insert(frame.anchor, (frame.start, output.position - frame.start));
                    }
                    after(&mut frames);
                }
                Event::Alias(anchor) => {
                    if frames.last().is_some_and(|frame| frame.map && frame.key) {
                        bail!("YAML alias keys are not supported");
                    }
                    before(&mut frames, &mut output)?;
                    let (start, length) =
                        anchors.get(&anchor).copied().ok_or_else(|| anyhow::anyhow!("recursive YAML aliases are not supported"))?;
                    output.flush()?;
                    alias_reader.seek(SeekFrom::Start(start))?;
                    io::copy(&mut (&mut alias_reader).take(length), &mut output)?;
                    after(&mut frames);
                }
                Event::Nothing | Event::StreamStart | Event::DocumentEnd => {}
            }
        }
        output.flush()?;
        Ok(())
    })();
    if let Some(error) = error.borrow_mut().take() {
        return Err(error.into());
    }
    result
}

fn before(frames: &mut [Frame], output: &mut Output) -> Result<()> {
    if let Some(frame) = frames.last()
        && (!frame.map || frame.key)
        && frame.count > 0
    {
        output.write_all(b",")?;
    }
    Ok(())
}

fn after(frames: &mut [Frame]) {
    if let Some(frame) = frames.last_mut() {
        frame.count += 1;
        if frame.map {
            frame.key = true;
        }
    }
}
