//! Render errors, so that none is lost among the log's lines again.
//!
//! An error is reported by its [`Kind`] and its context (the screen or
//! surface it happened on, the class of asset: `"F7 screen; sheet
//! screen_resized_region"`), with the details of this occurrence (which
//! leader, which tile, which file) apart from them. The kind and the
//! context make the error's type, its key: the first report of a type is
//! kept with its details and written to the render error log
//! (`civ3-render-errors.log`, next to `civ3-bridge.log`, [`log_to_file`]);
//! every report is counted, per type and in all (for the viewer's on-screen
//! notice, [`Rolling`] and [`Notice`]).
//!
//! # One run, both sides
//!
//! Each binary (the bridge DLL, the native unixlib, a viewer process) has
//! its own [`ERRORS`]: an in-process table keyed by a stable hash of the
//! type, per process and so per game run, never a file shared between
//! runs. A report is a few atomic operations: the hash, a lock-free insert
//! into a fixed open-addressed table, two counters, and only for a type new
//! to this side the formatted record sent through a lock-free mailbox. The
//! other sides hand their records and counts to the bridge with their log
//! lines ([`take_forward`], [`absorb`]); the bridge's log writer, the one
//! owner of the file, keeps each type once whichever side reported it
//! first. A new game run is a new process: everything starts empty, and
//! the log file is made anew.

use std::fmt;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use handoff::Mailbox;

mod file;
mod forward;
mod rolling;

pub use file::{FILE_NAME, Log, log_to_file};
pub use forward::{Forwarded, PREFIX, TARGET, absorb, decode, encode_count, encode_record, take_forward};
pub use rolling::{HOLD, Notice, REFRESH, Rolling, WINDOW, notice_text};

/// What went wrong, in general. The ids are stable (they cross between the
/// sides and are hashed into the types' keys): never renumber one.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Kind {
    /// The art description could not be read (its TOML, a name it refers to).
    ArtDescription = 1,
    /// An image the art takes could not be loaded or made.
    ArtImage = 2,
    /// A sheet of the art could not be cut from its images.
    ArtSheet = 3,
    /// The atlas had no room for the art, or refused a sprite.
    Atlas = 4,
    /// Art could not be handed to the renderer (the art thread gone, the
    /// renderer refused the call or the bytes).
    ArtHandOver = 5,
    /// Art built for the scenes was not adopted by them.
    ArtNotAdopted = 6,
    /// A sprite or sprite set a scene or the interface names is not in the
    /// art.
    MissingSprite = 7,
    /// A font the text asks for is not there.
    MissingFont = 8,
    /// Text has characters its font cannot draw.
    TextShape = 9,
    /// A draw of the game's that the mirror does not support.
    UnsupportedDraw = 10,
    /// The GPU failed: device, surface, shader, pipeline.
    Gpu = 11,
    /// Scene updates or input could not be encoded, decoded or delivered.
    SceneTransfer = 12,
    /// The viewer stopped, or could not start, with an error.
    Viewer = 13,
    /// The render worker refused a frame.
    RenderWorker = 14,
    /// Art a screen shows did not arrive; it is drawn as nothing.
    VisibleArtMissing = 15,
    /// Views were left out: more than the renderer draws at once.
    ViewsLeftOut = 16,
}

impl Kind {
    pub const ALL: [Kind; 16] = [
        Kind::ArtDescription,
        Kind::ArtImage,
        Kind::ArtSheet,
        Kind::Atlas,
        Kind::ArtHandOver,
        Kind::ArtNotAdopted,
        Kind::MissingSprite,
        Kind::MissingFont,
        Kind::TextShape,
        Kind::UnsupportedDraw,
        Kind::Gpu,
        Kind::SceneTransfer,
        Kind::Viewer,
        Kind::RenderWorker,
        Kind::VisibleArtMissing,
        Kind::ViewsLeftOut,
    ];

    pub fn id(self) -> u16 {
        self as u16
    }

    pub fn from_id(id: u16) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The kind in words, for the log.
    pub fn describe(self) -> &'static str {
        match self {
            Kind::ArtDescription => "the art description could not be read",
            Kind::ArtImage => "an image of the art could not be loaded",
            Kind::ArtSheet => "a sheet of the art could not be made",
            Kind::Atlas => "the atlas refused art",
            Kind::ArtHandOver => "art could not be handed to the renderer",
            Kind::ArtNotAdopted => "art was not adopted by the scenes",
            Kind::MissingSprite => "a sprite is not in the art",
            Kind::MissingFont => "a font is missing",
            Kind::TextShape => "text has characters its font cannot draw",
            Kind::UnsupportedDraw => "a mirrored draw is not supported",
            Kind::Gpu => "the GPU failed",
            Kind::SceneTransfer => "scene updates could not be delivered",
            Kind::Viewer => "the viewer failed",
            Kind::RenderWorker => "the render worker refused a frame",
            Kind::VisibleArtMissing => "art shown on screen did not arrive",
            Kind::ViewsLeftOut => "views were left out (too many to draw)",
        }
    }
}

/// The stable hash (64-bit FNV-1a, never 0) of the type `kind` in `context`:
/// the same on every side and in every build.
pub fn key_hash(kind: Kind, context: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in kind.id().to_le_bytes().into_iter().chain([0xff]).chain(context.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h.max(1)
}

/// A type's name in the log: its kind and its context.
pub fn title(kind: Kind, context: &str) -> String {
    if context.is_empty() { kind.describe().to_owned() } else { format!("{} ({context})", kind.describe()) }
}

/// The class of an asset named `name`, for a context: a file's folder
/// (`Art/Leaderheads/AL01.pcx` → `Art/Leaderheads`), else the name without
/// the numbers that tell its instances apart (`screen_resized_region_0` →
/// `screen_resized_region`).
pub fn class_of(name: &str) -> &str {
    let name = name.trim();
    if let Some(i) = name.rfind(['/', '\\']) {
        return &name[..i];
    }
    let class = name.trim_end_matches(|c: char| c.is_ascii_digit() || matches!(c, '_' | '-' | '#' | '.' | ' '));
    if class.is_empty() { name } else { class }
}

/// Where a record was first reported.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// In this binary.
    Here,
    /// The native unixlib's (forwarded with its log lines).
    Native,
    /// A viewer in a process of its own (forwarded over its connection).
    Viewer,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Here => "bridge",
            Origin::Native => "native",
            Origin::Viewer => "viewer",
        }
    }
}

/// The first report of a type, with the details it came with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub hash: u64,
    pub kind: Kind,
    pub context: String,
    pub detail: String,
    pub origin: Origin,
}

/// How many types one side tells apart (more are counted in all only).
pub const TYPES: usize = 1024;

/// The types seen and their counts: fixed tables of atomics, so that a
/// report never waits, allocates or formats for a type seen before.
pub struct Registry {
    /// A type's hash per slot (0: free), open addressed from `hash % TYPES`.
    keys: [AtomicU64; TYPES],
    /// Reports per slot (on every side, for the bridge's: forwarded ones
    /// added).
    counts: [AtomicU64; TYPES],
    /// Reports per slot not yet forwarded ([`Registry::take_counts`]).
    unforwarded: [AtomicU64; TYPES],
    /// Every report on this side.
    total: AtomicU64,
    /// Reports of types beyond [`TYPES`].
    overflow: AtomicU64,
    /// First reports of types, for the log's writer or the forwarding.
    new: LazyLock<Mailbox<Record>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub const fn new() -> Self {
        Registry {
            keys: [const { AtomicU64::new(0) }; TYPES],
            counts: [const { AtomicU64::new(0) }; TYPES],
            unforwarded: [const { AtomicU64::new(0) }; TYPES],
            total: AtomicU64::new(0),
            overflow: AtomicU64::new(0),
            new: LazyLock::new(Mailbox::new),
        }
    }

    /// The slot of `hash`, taken now if it had none (then `true`); `None`
    /// with every slot taken by other types.
    fn slot(&self, hash: u64) -> Option<(usize, bool)> {
        let start = (hash % TYPES as u64) as usize;
        for i in (start..TYPES).chain(0..start) {
            match self.keys[i].load(Relaxed) {
                h if h == hash => return Some((i, false)),
                0 => match self.keys[i].compare_exchange(0, hash, Relaxed, Relaxed) {
                    Ok(_) => return Some((i, true)),
                    Err(h) if h == hash => return Some((i, false)),
                    Err(_) => {}
                },
                _ => {}
            }
        }
        None
    }

    fn find(&self, hash: u64) -> Option<usize> {
        let start = (hash % TYPES as u64) as usize;
        (start..TYPES).chain(0..start).map(|i| (i, self.keys[i].load(Relaxed))).take_while(|&(_, h)| h != 0).find(|&(_, h)| h == hash).map(|(i, _)| i)
    }

    /// Reports an error of `kind` in `context`; `detail` is formatted only
    /// if its type is new here. Whether it was.
    pub fn report(&self, kind: Kind, context: &str, detail: fmt::Arguments<'_>) -> bool {
        self.total.fetch_add(1, Relaxed);
        let hash = key_hash(kind, context);
        let Some((i, new)) = self.slot(hash) else {
            self.overflow.fetch_add(1, Relaxed);
            return false;
        };
        self.counts[i].fetch_add(1, Relaxed);
        self.unforwarded[i].fetch_add(1, Relaxed);
        if new {
            self.new.send(Record { hash, kind, context: context.to_owned(), detail: detail.to_string(), origin: Origin::Here });
        }
        new
    }

    /// A first report from another side (its counts come apart,
    /// [`Registry::absorb_count`]): to be logged unless this side has it.
    pub fn absorb_record(&self, record: Record) {
        let _ = self.slot(record.hash);
        self.new.send(record);
    }

    /// `delta` more reports of type `hash` on another side.
    pub fn absorb_count(&self, hash: u64, delta: u64) {
        match self.slot(hash) {
            Some((i, _)) => self.counts[i].fetch_add(delta, Relaxed),
            None => self.overflow.fetch_add(delta, Relaxed),
        };
    }

    /// The first reports since the last call.
    pub fn take_records(&self) -> Vec<Record> {
        self.new.drain().collect()
    }

    /// Per type (hash), the reports since the last call.
    pub fn take_counts(&self) -> Vec<(u64, u64)> {
        (0..TYPES)
            .filter(|&i| self.unforwarded[i].load(Relaxed) > 0)
            .filter_map(|i| Some((self.keys[i].load(Relaxed), self.unforwarded[i].swap(0, Relaxed))).filter(|&(_, n)| n > 0))
            .collect()
    }

    /// Reports of type `hash`, on every side this one knows of.
    pub fn count_of(&self, hash: u64) -> u64 {
        self.find(hash).map_or(0, |i| self.counts[i].load(Relaxed))
    }

    /// Every report on this side.
    pub fn total(&self) -> u64 {
        self.total.load(Relaxed)
    }

    /// Reports of types beyond [`TYPES`].
    pub fn overflow(&self) -> u64 {
        self.overflow.load(Relaxed)
    }
}

/// This binary's render errors.
pub static ERRORS: Registry = Registry::new();

/// Reports an error of `kind` in `context` (no variable details: those go
/// in `detail`, formatted only for a type new to this side). Lock-free, for
/// any thread (the game's, the render thread). Whether the type was new.
///
/// `render_errors::report(Kind::ArtSheet, "F7 screen; sheet portrait", format_args!("{error}"))`
pub fn report(kind: Kind, context: &str, detail: fmt::Arguments<'_>) -> bool {
    ERRORS.report(kind, context, detail)
}

/// Every error reported on this side so far (the viewer's notice counts its
/// increase).
pub fn total() -> u64 {
    ERRORS.total()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_is_its_kind_and_context_not_its_details() {
        let r = Registry::new();
        assert!(r.report(Kind::ArtImage, "leader portraits; F4 screen", format_args!("the Greek leader failed to load")));
        assert!(!r.report(Kind::ArtImage, "leader portraits; F4 screen", format_args!("the Roman leader failed to load")));
        // Another screen, another type; another kind, too.
        assert!(r.report(Kind::ArtImage, "leader portraits; diplomacy screen", format_args!("the Greek leader failed to load")));
        assert!(r.report(Kind::ArtSheet, "leader portraits; F4 screen", format_args!("x")));
        let records = r.take_records();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].detail, "the Greek leader failed to load", "the first occurrence's details are kept");
        assert_eq!(r.count_of(key_hash(Kind::ArtImage, "leader portraits; F4 screen")), 2);
        assert_eq!(r.total(), 4);
        assert!(r.take_records().is_empty());
    }

    #[test]
    fn a_new_run_reports_again() {
        // A run is a process: its registry starts empty.
        for _ in 0..2 {
            let r = Registry::new();
            assert!(r.report(Kind::Gpu, "surface", format_args!("lost")));
            assert_eq!(r.take_records().len(), 1);
        }
    }

    #[test]
    fn hashes_are_stable_and_never_zero() {
        // (Both sides and every build agree: a fixed value.)
        assert_eq!(key_hash(Kind::ArtSheet, "F7 screen"), key_hash(Kind::ArtSheet, "F7 screen"));
        assert_ne!(key_hash(Kind::ArtSheet, "F7 screen"), key_hash(Kind::ArtImage, "F7 screen"));
        assert_eq!(key_hash(Kind::ArtSheet, "F7 screen"), 0x8c3c_ead9_8509_9944, "a fixed value: never change the hash");
        for k in Kind::ALL {
            assert_eq!(Kind::from_id(k.id()), Some(k));
            assert_ne!(key_hash(k, ""), 0);
        }
    }

    #[test]
    fn counts_are_forwarded_once_and_overflow_is_counted() {
        let r = Registry::new();
        for _ in 0..3 {
            r.report(Kind::MissingSprite, "units", format_args!("a"));
        }
        assert_eq!(r.take_counts(), vec![(key_hash(Kind::MissingSprite, "units"), 3)]);
        assert!(r.take_counts().is_empty());
        for i in 0..TYPES + 5 {
            r.report(Kind::MissingSprite, &format!("ctx {i}"), format_args!(""));
        }
        assert_eq!(r.overflow(), 6, "one slot was taken before");
    }

    #[test]
    fn many_threads_report_one_type_once() {
        let r = Registry::new();
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..1000 {
                        r.report(Kind::TextShape, "chat", format_args!("x"));
                    }
                });
            }
        });
        assert_eq!(r.take_records().len(), 1);
        assert_eq!(r.count_of(key_hash(Kind::TextShape, "chat")), 8000);
    }

    #[test]
    fn classes_drop_what_tells_instances_apart() {
        assert_eq!(class_of("screen_resized_region_0"), "screen_resized_region");
        assert_eq!(class_of("Art/Leaderheads/AL01.pcx"), "Art/Leaderheads");
        assert_eq!(class_of("Art\\Units\\Warrior\\Warrior.ini"), "Art\\Units\\Warrior");
        assert_eq!(class_of("Viewer@busy_panel"), "Viewer@busy_panel");
        assert_eq!(class_of("42"), "42");
    }
}
