use std::fmt::Display;
use std::io::Write as _;
use std::path::PathBuf;

pub fn report_error(context: &str, error: impl Display) {
    let line = format!("{} {context}: {error:#}", chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    eprintln!("kestrel: {line}");
    append_to_log(&line);
}

pub trait ReportExt<T> {
    fn report_err(self, context: &str) -> Option<T>;
}

impl<T, E: Display> ReportExt<T> for Result<T, E> {
    fn report_err(self, context: &str) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(error) => {
                report_error(context, error);
                None
            }
        }
    }
}

pub fn log_path() -> Option<PathBuf> {
    Some(dirs::state_dir().or_else(dirs::data_local_dir)?.join("kestrel").join("errors.log"))
}

fn append_to_log(line: &str) {
    let Some(path) = log_path() else { return };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::OpenOptions::new().create(true).append(true).open(&path))
        .and_then(|mut file| writeln!(file, "{line}"));
    if let Err(error) = written {
        eprintln!("kestrel: could not write {}: {error}", path.display());
    }
}
