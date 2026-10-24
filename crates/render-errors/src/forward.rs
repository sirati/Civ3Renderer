//! Records and counts of another side, as text lines: a native side's
//! go with its log lines (target [`TARGET`]), a
//! viewer process's with its error messages (`viewer_transport`). The
//! bridge takes them into its [`ERRORS`] ([`absorb`]).

use super::{ERRORS, Kind, Origin, Record, Registry};

/// The log target of forwarded lines.
pub const TARGET: &str = "render_errors_forwarded";

/// The start of every forwarded line.
pub const PREFIX: &str = "\u{1}render-error\t";

/// What a forwarded line says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Forwarded {
    Record(Record),
    /// More reports of a type: (hash, how many).
    Count(u64, u64),
}

fn field(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

pub fn encode_record(r: &Record) -> String {
    format!("{PREFIX}r\t{:016x}\t{}\t{}\t{}", r.hash, r.kind.id(), field(&r.context), field(&r.detail))
}

pub fn encode_count(hash: u64, delta: u64) -> String {
    format!("{PREFIX}c\t{hash:016x}\t{delta}")
}

/// The line `line` from `origin`; `None` if it is no forwarded line.
pub fn decode(line: &str, origin: Origin) -> Option<Forwarded> {
    let mut f = line.strip_prefix(PREFIX)?.splitn(5, '\t');
    let tag = f.next()?;
    let hash = u64::from_str_radix(f.next()?, 16).ok()?;
    match tag {
        "r" => {
            let kind = Kind::from_id(f.next()?.parse().ok()?)?;
            let (context, detail) = (f.next()?.to_owned(), f.next().unwrap_or_default().to_owned());
            Some(Forwarded::Record(Record { hash, kind, context, detail, origin }))
        }
        "c" => Some(Forwarded::Count(hash, f.next()?.parse().ok()?)),
        _ => None,
    }
}

/// What `registry` has to forward since the last call: first reports, then
/// counts.
pub fn take_forward_from(registry: &Registry) -> Vec<String> {
    let mut out: Vec<String> = registry.take_records().iter().map(encode_record).collect();
    out.extend(registry.take_counts().into_iter().map(|(h, n)| encode_count(h, n)));
    out
}

/// What this side's [`ERRORS`] has to forward since the last call.
pub fn take_forward() -> Vec<String> {
    take_forward_from(&ERRORS)
}

/// Takes `line` from `origin` into `registry` if it is a forwarded line
/// (whether it was).
pub fn absorb_into(registry: &Registry, line: &str, origin: Origin) -> bool {
    match decode(line, origin) {
        Some(Forwarded::Record(r)) => registry.absorb_record(r),
        Some(Forwarded::Count(h, n)) => registry.absorb_count(h, n),
        None => return false,
    }
    true
}

/// Takes `line` into this side's [`ERRORS`] if it is a forwarded line.
pub fn absorb(line: &str, origin: Origin) -> bool {
    absorb_into(&ERRORS, line, origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_hash;

    #[test]
    fn lines_survive_the_trip() {
        let r = Record { hash: 77, kind: Kind::ArtSheet, context: "F7\tscreen".into(), detail: "a\nb".into(), origin: Origin::Here };
        let back = decode(&encode_record(&r), Origin::Native).unwrap();
        assert_eq!(back, Forwarded::Record(Record { context: "F7 screen".into(), detail: "a b".into(), origin: Origin::Native, ..r }));
        assert_eq!(decode(&encode_count(5, 9), Origin::Native), Some(Forwarded::Count(5, 9)));
        assert_eq!(decode("art 93: sheet", Origin::Native), None);
        assert_eq!(decode(&format!("{PREFIX}r\t1\t999\tx\ty"), Origin::Native), None, "an unknown kind");
    }

    #[test]
    fn both_sides_land_in_the_bridges_registry_once_per_type() {
        let (native, bridge) = (Registry::new(), Registry::new());
        for portrait in ["first", "second"] {
            native.report(Kind::ArtImage, "portraits; report screen", format_args!("the {portrait} portrait"));
        }
        bridge.report(Kind::ArtImage, "portraits; report screen", format_args!("on the bridge"));
        native.report(Kind::Gpu, "surface", format_args!("lost"));
        for line in take_forward_from(&native) {
            assert!(absorb_into(&bridge, &line, Origin::Native));
        }
        let records = bridge.take_records();
        // The bridge's own, then the native side's two (the log's writer
        // keeps the first of each hash, `Log`).
        assert_eq!(records.len(), 3);
        assert_eq!(bridge.count_of(key_hash(Kind::ArtImage, "portraits; report screen")), 3);
        assert_eq!(bridge.count_of(key_hash(Kind::Gpu, "surface")), 1);
        // Forwarded reports do not count as the bridge's own (the viewer
        // counts the native side's itself).
        assert_eq!(bridge.total(), 1);
    }
}
