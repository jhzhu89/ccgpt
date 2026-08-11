use std::{
    fs::{File, OpenOptions},
    io::{LineWriter, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value};

const MAX_DETAIL_CHARS: usize = 2_000;

#[derive(Clone, Default)]
pub(crate) struct Diagnostics {
    inner: Option<Arc<DiagnosticWriter>>,
}

struct DiagnosticWriter {
    writer: Mutex<LineWriter<File>>,
    reported_failure: AtomicBool,
}

impl Diagnostics {
    pub(crate) fn open(path: Option<&Path>) -> Result<Self, String> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let file = open_file(path).map_err(|error| {
            format!("failed to open diagnostic log {}: {error}", path.display())
        })?;
        Ok(Self {
            inner: Some(Arc::new(DiagnosticWriter {
                writer: Mutex::new(LineWriter::new(file)),
                reported_failure: AtomicBool::new(false),
            })),
        })
    }

    pub(crate) fn record(&self, event: &str, fields: Value) {
        let Some(inner) = &self.inner else {
            return;
        };
        let mut record = Map::new();
        record.insert("timestamp_ms".into(), Value::from(timestamp_ms()));
        record.insert("event".into(), Value::from(event));
        if let Value::Object(fields) = fields {
            record.extend(fields);
        }
        let line = Value::Object(record).to_string();
        let result = inner
            .writer
            .lock()
            .map_err(|_| "diagnostic log lock was poisoned".to_owned())
            .and_then(|mut writer| writeln!(writer, "{line}").map_err(|error| error.to_string()));
        if let Err(error) = result
            && !inner.reported_failure.swap(true, Ordering::Relaxed)
        {
            eprintln!("ccgpt: failed to write diagnostic log: {error}");
        }
    }
}

pub(crate) fn detail(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    let mut chars = value.chars();
    let mut detail = chars.by_ref().take(MAX_DETAIL_CHARS).collect::<String>();
    if chars.next().is_some() {
        detail.push('…');
    }
    detail
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn open_file(path: &Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        let file = options.open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        return Ok(file);
    }
    #[cfg(not(unix))]
    return options.open(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writes_json_lines_and_limits_details() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("diagnostics.jsonl");
        let diagnostics = Diagnostics::open(Some(&path)).unwrap();
        diagnostics.record(
            "upstream_stream_failed",
            json!({ "detail": detail("x".repeat(MAX_DETAIL_CHARS + 10)) }),
        );

        let line = std::fs::read_to_string(path).unwrap();
        let record: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(record["event"], "upstream_stream_failed");
        assert_eq!(
            record["detail"].as_str().unwrap().chars().count(),
            MAX_DETAIL_CHARS + 1
        );
        assert!(record["timestamp_ms"].as_u64().unwrap() > 0);
    }
}
