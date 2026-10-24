//! The counting mode's report: how often each slot of each class was
//! called (`docs/jgl-replacement.md` §7).

use crate::slots::{Class, Group};

/// Call counts, one per slot, per class in [`Class::ALL`] order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub per_class: [Vec<u32>; 5],
}

impl Counts {
    pub fn zero() -> Counts {
        Counts { per_class: Class::ALL.map(|c| vec![0; c.slots().len()]) }
    }

    pub fn class(&self, class: Class) -> &[u32] {
        &self.per_class[Class::ALL.iter().position(|&c| c == class).unwrap_or(0)]
    }

    /// Lines `class+0xOFF name group count`, of the slots called, most
    /// called first; then one line per class with the slots never called
    /// (stubs left out).
    pub fn report(&self) -> String {
        let mut called = Vec::new();
        let mut never = Vec::new();
        for (class, counts) in Class::ALL.iter().zip(&self.per_class) {
            let mut idle = Vec::new();
            for (slot, &n) in class.slots().iter().zip(counts) {
                if n > 0 {
                    called.push((n, format!("{}+{:#05x} {} {:?}", class.name(), slot.offset, slot.name, slot.group)));
                } else if slot.group != Group::Stub {
                    idle.push(format!("{:#x}", slot.offset));
                }
            }
            never.push(format!("{} never: {}", class.name(), idle.join(" ")));
        }
        called.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let mut out = String::new();
        for (n, what) in called {
            out.push_str(&format!("{what} {n}\n"));
        }
        for line in never {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_lists_called_slots_by_count_and_the_idle_rest() {
        let mut c = Counts::zero();
        c.per_class[1][0xB8 / 4] = 40;
        c.per_class[1][0xD8 / 4] = 900;
        c.per_class[4][0x10 / 4] = 3;
        let r = c.report();
        let lines: Vec<&str> = r.lines().collect();
        assert_eq!(lines[0], "image+0x0d8 width Surface 900");
        assert_eq!(lines[1], "image+0x0b8 text_out Text 40");
        assert_eq!(lines[2], "font+0x010 measure Text 3");
        assert!(lines.iter().any(|l| l.starts_with("font never: 0x0 0x4 0x8 0xc")));
        // Stubs are not listed as idle.
        assert!(!lines.iter().any(|l| l.starts_with("image never:") && l.contains(" 0x68 ")));
    }
}
