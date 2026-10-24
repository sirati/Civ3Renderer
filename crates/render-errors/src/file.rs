//! The render error log: made anew for each game run, one line per type of
//! error (the first time it happened, on whichever side), then the counts
//! of every type so far. A writer thread of its own owns it; reporters
//! never wait for it (it polls the registry's mailbox and counters).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{ERRORS, Kind, Origin, Record, Registry, title};


/// How often the writer looks for new reports.
const POLL: Duration = Duration::from_millis(100);
/// Counts alone are rewritten at most this often.
const COUNTS_EVERY: Duration = Duration::from_secs(1);

struct Entry {
    record: Record,
    /// When it came, since the run started.
    at: Duration,
    /// The other sides that reported it too.
    also: Vec<Origin>,
}

/// The log's contents: the types in the order they first came.
pub struct Log {
    start: Instant,
    entries: Vec<Entry>,
    seen: HashMap<u64, usize>,
    /// The file's name, for the main log.
    name: String,
    /// The counts last written, and when.
    written: Option<(Vec<u64>, u64, Instant)>,
}

impl Log {
    /// For the file named `name`.
    pub fn new(start: Instant, name: &str) -> Self {
        Log { start, name: name.to_owned(), entries: Vec::new(), seen: HashMap::new(), written: None }
    }

    /// Takes the registry's new records at `now`: the types not seen before
    /// (also logged once to the main log). Returns the file's new text when
    /// it is to be written (a new type at once, changed counts at most every
    /// second).
    pub fn poll(&mut self, registry: &Registry, now: Instant) -> Option<String> {
        let mut new = false;
        for record in registry.take_records() {
            match self.seen.get(&record.hash) {
                Some(&i) => {
                    let e = &mut self.entries[i];
                    if record.origin != e.record.origin && !e.also.contains(&record.origin) {
                        e.also.push(record.origin);
                        new = true;
                    }
                }
                None => {
                    log::error!(target: "render_error", "{} [{}]: {} (first of its kind; {})", title(record.kind, &record.context), record.origin.name(), record.detail, self.name);
                    self.seen.insert(record.hash, self.entries.len());
                    self.entries.push(Entry { record, at: now.saturating_duration_since(self.start), also: Vec::new() });
                    new = true;
                }
            }
        }
        let counts: Vec<u64> = self.entries.iter().map(|e| registry.count_of(e.record.hash)).collect();
        let overflow = registry.overflow();
        let due = match &self.written {
            None => true,
            Some((c, o, at)) => new || ((*c != counts || *o != overflow) && now.saturating_duration_since(*at) >= COUNTS_EVERY),
        };
        if !due {
            return None;
        }
        let text = self.render(&counts, overflow, now);
        self.written = Some((counts, overflow, now));
        Some(text)
    }

    fn render(&self, counts: &[u64], overflow: u64, now: Instant) -> String {
        let mut out = String::from(
            "Render errors of this game run. Each type of error (its kind and context) is listed once, the first time it\n\
             happened, with that occurrence's details; the counts at the end include every repeat.\n\n",
        );
        if self.entries.is_empty() {
            out.push_str("(none so far)\n");
        }
        for e in &self.entries {
            let r = &e.record;
            let also = if e.also.is_empty() { String::new() } else { format!(" (also {})", e.also.iter().map(|o| o.name()).collect::<Vec<_>>().join(", ")) };
            let _ = writeln!(out, "{:>8.3} s [{}{also}] {}: {}", e.at.as_secs_f64(), r.origin.name(), title(r.kind, &r.context), r.detail);
        }
        let total: u64 = counts.iter().sum::<u64>() + overflow;
        let _ = writeln!(out, "\nCounts after {:.1} s: {} types, {total} errors", now.saturating_duration_since(self.start).as_secs_f64(), self.entries.len());
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(counts[i]));
        for i in order {
            let r = &self.entries[i].record;
            let _ = writeln!(out, "{:>10}  {}", counts[i], title(r.kind, &r.context));
        }
        if overflow > 0 {
            let _ = writeln!(out, "{overflow:>10}  (of types beyond the {} told apart)", super::TYPES);
        }
        out
    }

    /// The kinds logged so far, in order (for tests).
    pub fn kinds(&self) -> Vec<(Kind, &str)> {
        self.entries.iter().map(|e| (e.record.kind, e.record.context.as_str())).collect()
    }
}

fn write(path: &Path, text: &str) {
    // (Whole or not at all: a crash between keeps the last complete one.)
    let temporary = path.with_extension("log.tmp");
    let replaced = std::fs::write(&temporary, text).and_then(|()| std::fs::rename(&temporary, path));
    if replaced.is_err() {
        let _ = std::fs::remove_file(&temporary);
        let _ = std::fs::write(path, text);
    }
}

/// Writes this side's [`ERRORS`] (with what other sides forwarded into it)
/// to `path`, made anew now, on a thread of its own for the rest of the
/// process. Call once, at the start of a game run.
pub fn log_to_file(path: &Path) -> std::io::Result<()> {
    let start = Instant::now();
    let mut log = Log::new(start, &path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()));
    let path: PathBuf = path.to_owned();
    if let Some(text) = log.poll(&ERRORS, start) {
        std::fs::write(&path, text)?;
    }
    std::thread::Builder::new().name("render-errors".into()).spawn(move || {
        loop {
            std::thread::sleep(POLL);
            if let Some(text) = log.poll(&ERRORS, Instant::now()) {
                write(&path, &text);
            }
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forward::{absorb_into, take_forward_from};

    #[test]
    fn each_type_is_logged_once_with_the_first_details_and_counted() {
        let (bridge, native) = (Registry::new(), Registry::new());
        let t0 = Instant::now();
        let mut log = Log::new(t0, "render-errors.log");
        let first = log.poll(&bridge, t0).unwrap();
        assert!(first.contains("(none so far)"));
        assert_eq!(log.poll(&bridge, t0), None, "nothing new");
        for portrait in ["first", "second", "third"] {
            bridge.report(Kind::ArtImage, "portraits; report screen", format_args!("the {portrait} portrait failed to load"));
        }
        native.report(Kind::ArtSheet, "F7 screen; sheet screen_resized_region", format_args!("an 8-bit indexed image cannot be rescaled"));
        native.report(Kind::ArtSheet, "F8 screen; sheet screen_resized_region", format_args!("the same, elsewhere"));
        native.report(Kind::ArtImage, "portraits; report screen", format_args!("the first portrait, natively"));
        for line in take_forward_from(&native) {
            absorb_into(&bridge, &line, Origin::Native);
        }
        let text = log.poll(&bridge, t0 + Duration::from_millis(100)).unwrap();
        assert_eq!(log.kinds(), [
            (Kind::ArtImage, "portraits; report screen"),
            (Kind::ArtSheet, "F7 screen; sheet screen_resized_region"),
            (Kind::ArtSheet, "F8 screen; sheet screen_resized_region"),
        ]);
        assert!(text.contains("[bridge (also native)] an image of the art could not be loaded (portraits; report screen): the first portrait failed to load"), "{text}");
        assert!(!text.contains("second portrait"), "later details are not kept");
        assert!(text.contains("         4  an image of the art could not be loaded (portraits; report screen)"), "{text}");
        assert!(text.contains("3 types, 6 errors"), "{text}");
        // Counts alone: at most once a second.
        bridge.report(Kind::ArtImage, "portraits; report screen", format_args!("again"));
        assert_eq!(log.poll(&bridge, t0 + Duration::from_millis(500)), None);
        assert!(log.poll(&bridge, t0 + Duration::from_millis(1200)).unwrap().contains("         5  an image"));
    }

    #[test]
    fn the_file_is_made_anew() {
        let dir = std::env::temp_dir().join(format!("render-errors-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("render-errors.log");
        std::fs::write(&path, "the last run's").unwrap();
        let mut log = Log::new(Instant::now(), "render-errors.log");
        write(&path, &log.poll(&Registry::new(), Instant::now()).unwrap());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("the last run's") && text.contains("(none so far)"));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
