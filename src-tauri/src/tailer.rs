//! Incremental JSONL readers: only bytes appended since the last read.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::transcript::{parse_line, parse_text, UsageLine};

/// Bytes read per call, so a huge backlog never lands in memory at once.
const READ_CHUNK: u64 = 4 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct FileTail {
    offset: u64,
    partial: Vec<u8>,
}

impl FileTail {
    /// Returns complete lines appended since the last call. A trailing
    /// partial line is kept for the next call. A truncated file restarts.
    pub fn read_new(&mut self, path: &Path) -> io::Result<Vec<String>> {
        let mut f = File::open(path)?;
        let len = f.metadata()?.len();
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        if len == self.offset {
            return Ok(Vec::new());
        }
        f.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        f.take(READ_CHUNK).read_to_end(&mut buf)?;
        self.offset += buf.len() as u64;

        self.partial.extend_from_slice(&buf);
        let Some(last_nl) = self.partial.iter().rposition(|&b| b == b'\n') else {
            return Ok(Vec::new());
        };
        let rest = self.partial.split_off(last_nl + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        Ok(String::from_utf8_lossy(&complete)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Reads until the file is exhausted (for large backlogs).
    pub fn read_all_new(&mut self, path: &Path) -> io::Result<Vec<String>> {
        let mut out = Vec::new();
        loop {
            let before = self.offset;
            out.extend(self.read_new(path)?);
            if self.offset == before || self.offset - before < READ_CHUNK {
                return Ok(out);
            }
        }
    }
}

/// Latest context usage per active transcript.
#[derive(Debug, Default)]
pub struct ContextTailer {
    files: HashMap<PathBuf, (FileTail, Option<UsageLine>)>,
}

/// Result of one poll.
#[derive(Debug, Default)]
pub struct TailPoll {
    /// Latest main-thread usage seen so far.
    pub usage: Option<UsageLine>,
    /// Newest assistant text among the lines read by this poll only.
    pub text: Option<String>,
}

impl ContextTailer {
    /// Reads appended lines: latest main-thread usage and newest assistant text.
    pub fn poll(&mut self, path: &Path) -> TailPoll {
        let (tail, latest) = self.files.entry(path.to_path_buf()).or_default();
        let lines = tail.read_all_new(path).unwrap_or_default();
        if let Some(u) = lines.iter().rev().filter_map(|l| parse_line(l)).find(|u| !u.sidechain) {
            *latest = Some(u);
        }
        TailPoll { usage: latest.clone(), text: lines.iter().rev().find_map(|l| parse_text(l)) }
    }

    /// Stops tracking transcripts not in `keep`.
    pub fn retain(&mut self, keep: &[PathBuf]) {
        self.files.retain(|p, _| keep.contains(p));
    }
}

const BUCKET_MS: u64 = 5 * 60 * 1000;
pub const FIVE_HOURS_MS: u64 = 5 * 60 * 60 * 1000;
pub const SEVEN_DAYS_MS: u64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Default)]
struct EstimateFile {
    tail: FileTail,
    last_msg_id: Option<String>,
    buckets: BTreeMap<u64, u64>,
}

/// Local plan usage estimate from every transcript under a root.
#[derive(Debug, Default)]
pub struct EstimateScanner {
    files: HashMap<PathBuf, EstimateFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EstimateTotals {
    pub five_hour: u64,
    pub seven_day: u64,
}

impl EstimateScanner {
    /// Rescans `root` (e.g. `~/.claude/projects`) and returns window totals.
    pub fn scan(&mut self, root: &Path, now_ms: u64) -> EstimateTotals {
        let cutoff = SystemTime::UNIX_EPOCH + Duration::from_millis(now_ms.saturating_sub(SEVEN_DAYS_MS));
        let mut seen = Vec::new();
        for path in jsonl_files(root) {
            let recent = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .is_ok_and(|t| t >= cutoff);
            if !recent {
                continue;
            }
            let file = self.files.entry(path.clone()).or_default();
            if let Ok(lines) = file.tail.read_all_new(&path) {
                for u in lines.iter().filter_map(|l| parse_line(l)) {
                    // Streaming writes repeat one message's usage on consecutive lines.
                    if u.msg_id.is_some() && u.msg_id == file.last_msg_id {
                        continue;
                    }
                    file.last_msg_id = u.msg_id.clone();
                    if let Some(ts) = u.ts_ms {
                        let b = file.buckets.entry(ts / BUCKET_MS).or_default();
                        *b = b.saturating_add(u.billable_tokens());
                    }
                }
            }
            seen.push(path);
        }
        self.files.retain(|p, _| seen.contains(p));

        let week_start = now_ms.saturating_sub(SEVEN_DAYS_MS) / BUCKET_MS;
        let five_start = now_ms.saturating_sub(FIVE_HOURS_MS) / BUCKET_MS;
        let mut totals = EstimateTotals::default();
        for f in self.files.values_mut() {
            f.buckets.retain(|b, _| *b >= week_start);
            for (b, tokens) in &f.buckets {
                totals.seven_day = totals.seven_day.saturating_add(*tokens);
                if *b >= five_start {
                    totals.five_hour = totals.five_hour.saturating_add(*tokens);
                }
            }
        }
        totals
    }
}

fn jsonl_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(p),
                Ok(t) if t.is_file() && p.extension().is_some_and(|x| x == "jsonl") => out.push(p),
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn usage_line(id: &str, iso: &str, input: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{iso}","message":{{"id":"{id}","usage":{{"input_tokens":{input},"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0}}}}}}"#
        )
    }

    #[test]
    fn reads_only_appended() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        std::fs::write(&p, "a\nb\n").unwrap();
        let mut t = FileTail::default();
        assert_eq!(t.read_new(&p).unwrap(), vec!["a", "b"]);
        assert!(t.read_new(&p).unwrap().is_empty());
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(b"c\n").unwrap();
        assert_eq!(t.read_new(&p).unwrap(), vec!["c"]);
    }

    #[test]
    fn partial_line_kept_for_next_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        std::fs::write(&p, "a\npar").unwrap();
        let mut t = FileTail::default();
        assert_eq!(t.read_new(&p).unwrap(), vec!["a"]);
        std::fs::OpenOptions::new().append(true).open(&p).unwrap().write_all(b"tial\n").unwrap();
        assert_eq!(t.read_new(&p).unwrap(), vec!["partial"]);
    }

    #[test]
    fn truncation_resets() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        std::fs::write(&p, "aaaa\nbbbb\n").unwrap();
        let mut t = FileTail::default();
        t.read_new(&p).unwrap();
        std::fs::write(&p, "c\n").unwrap();
        assert_eq!(t.read_new(&p).unwrap(), vec!["c"]);
    }

    #[test]
    fn context_tailer_skips_sidechain() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        let main = usage_line("m1", "2026-09-30T10:00:00Z", 5000);
        let side = usage_line("m2", "2026-09-30T10:00:01Z", 9).replace(r#""type":"assistant""#, r#""type":"assistant","isSidechain":true"#);
        std::fs::write(&p, format!("{main}\n{side}\n")).unwrap();
        let mut ct = ContextTailer::default();
        assert_eq!(ct.poll(&p).usage.unwrap().context_tokens(), 5000);
        // No new lines: latest value is remembered.
        assert_eq!(ct.poll(&p).usage.unwrap().context_tokens(), 5000);
    }

    #[test]
    fn estimate_windows() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("proj");
        std::fs::create_dir(&proj).unwrap();
        let now = crate::transcript::parse_iso_ms("2026-09-30T12:00:00Z").unwrap();
        let lines = [
            usage_line("a", "2026-09-30T11:00:00Z", 100), // in 5h
            usage_line("a", "2026-09-30T11:00:00Z", 100), // duplicate streaming line
            usage_line("b", "2026-09-30T05:00:00Z", 10),  // in 7d only
            usage_line("c", "2026-09-20T05:00:00Z", 1),   // too old
        ];
        std::fs::write(proj.join("s.jsonl"), lines.join("\n") + "\n").unwrap();
        let mut sc = EstimateScanner::default();
        let t = sc.scan(dir.path(), now);
        assert_eq!(t, EstimateTotals { five_hour: 100, seven_day: 110 });
        // Rescan with nothing new gives the same totals.
        assert_eq!(sc.scan(dir.path(), now), t);
        // Time passes: the 11:00 bucket leaves the 5h window.
        let later = now + 5 * 60 * 60 * 1000;
        assert_eq!(sc.scan(dir.path(), later).five_hour, 0);
    }
}
