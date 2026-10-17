//! The native side's log lines, for the driver to write into its own log:
//! the native side's standard error is Wine's, which a game's player does not
//! keep (and which the driver's log should not need beside it).

use std::collections::VecDeque;

/// One log record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub level: log::Level,
    pub target: String,
    pub message: String,
}

/// Lines waiting for the driver: a line that repeats the one before is
/// counted rather than kept (a warning per frame becomes one line and a
/// count); beyond `cap` lines the oldest are dropped and counted.
#[derive(Debug)]
pub struct LogQueue {
    lines: VecDeque<LogLine>,
    /// How often the last line came again since it was queued.
    repeats: u64,
    dropped: u64,
    cap: usize,
}

impl LogQueue {
    pub const fn new(cap: usize) -> Self {
        Self { lines: VecDeque::new(), repeats: 0, dropped: 0, cap }
    }

    pub fn push(&mut self, line: LogLine) {
        // (While repeats are pending, the last line is the one repeated.)
        if self.lines.back() == Some(&line) {
            self.repeats += 1;
            return;
        }
        self.flush_repeats();
        self.lines.push_back(line);
        while self.lines.len() > self.cap.max(1) {
            self.lines.pop_front();
            self.dropped += 1;
        }
    }

    fn flush_repeats(&mut self) {
        if self.repeats == 0 {
            return;
        }
        let Some(last) = self.lines.back() else { return };
        let note = LogLine { message: format!("(the line before {} more times)", self.repeats), ..last.clone() };
        self.repeats = 0;
        self.lines.push_back(note);
    }

    /// Everything queued, oldest first (with a note of what was dropped or
    /// repeated meanwhile).
    pub fn take(&mut self) -> Vec<LogLine> {
        self.flush_repeats();
        let mut out = Vec::with_capacity(self.lines.len() + 1);
        if self.dropped > 0 {
            out.push(LogLine { level: log::Level::Warn, target: "civ3r_native".into(), message: format!("({} log lines dropped)", self.dropped) });
            self.dropped = 0;
        }
        out.extend(self.lines.drain(..));
        out
    }
}

/// `lines` as bytes: per line its level (1 error .. 5 trace), then target
/// and message, each a little-endian `u32` length and UTF-8.
pub fn encode(lines: &[LogLine]) -> Vec<u8> {
    let mut out = Vec::new();
    for l in lines {
        out.push(l.level as u8);
        for s in [&l.target, &l.message] {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
    }
    out
}

/// The lines of [`encode`]; `None` if the bytes are not such.
pub fn decode(mut bytes: &[u8]) -> Option<Vec<LogLine>> {
    let mut out = Vec::new();
    let take = |n: usize, bytes: &mut &[u8]| -> Option<Vec<u8>> {
        let (head, rest) = bytes.split_at_checked(n)?;
        *bytes = rest;
        Some(head.to_vec())
    };
    while !bytes.is_empty() {
        let level = match take(1, &mut bytes)?[0] {
            1 => log::Level::Error,
            2 => log::Level::Warn,
            3 => log::Level::Info,
            4 => log::Level::Debug,
            5 => log::Level::Trace,
            _ => return None,
        };
        let mut text = || -> Option<String> {
            let len = u32::from_le_bytes(take(4, &mut bytes)?.try_into().ok()?) as usize;
            String::from_utf8(take(len, &mut bytes)?).ok()
        };
        let (target, message) = (text()?, text()?);
        out.push(LogLine { level, target, message });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(level: log::Level, message: &str) -> LogLine {
        LogLine { level, target: "tile_viewer".into(), message: message.into() }
    }

    #[test]
    fn lines_survive_the_trip() {
        let lines = vec![line(log::Level::Info, "viewer adapter: Ä (Vulkan)"), line(log::Level::Error, ""), line(log::Level::Trace, "x")];
        assert_eq!(decode(&encode(&lines)), Some(lines));
        assert_eq!(decode(&[]), Some(vec![]));
        assert_eq!(decode(&[3, 1, 0, 0]), None);
        assert_eq!(decode(&[9, 0, 0, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn repeats_are_counted_and_old_lines_dropped() {
        let mut q = LogQueue::new(4);
        let spam = line(log::Level::Warn, "9 views, only the first 8 are drawn");
        q.push(line(log::Level::Info, "a"));
        for _ in 0..5 {
            q.push(spam.clone());
        }
        q.push(line(log::Level::Info, "b"));
        assert_eq!(q.take(), vec![
            line(log::Level::Info, "a"),
            spam.clone(),
            line(log::Level::Warn, "(the line before 4 more times)"),
            line(log::Level::Info, "b")
        ]);
        assert_eq!(q.take(), vec![]);
        // Repeats still pending when taken.
        q.push(spam.clone());
        q.push(spam.clone());
        assert_eq!(q.take(), vec![spam.clone(), line(log::Level::Warn, "(the line before 1 more times)")]);
        for m in ["1", "2", "3", "4", "5", "6"] {
            q.push(line(log::Level::Info, m));
        }
        let taken = q.take();
        assert_eq!(taken[0].message, "(2 log lines dropped)");
        assert_eq!(taken[1..].iter().map(|l| l.message.as_str()).collect::<Vec<_>>(), ["3", "4", "5", "6"]);
    }
}
