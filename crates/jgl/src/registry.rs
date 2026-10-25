//! jgl's circular object registry, recovered from 0x14B0, 0x1350 and
//! 0x1410. The head stays put on insertion; both head and release cursor
//! advance to the removed object's successor on every removal.

use std::collections::HashMap;

#[derive(Clone, Copy)]
struct Links { next: u32, prev: u32 }

#[derive(Default)]
pub struct Registry {
    nodes: HashMap<u32, Links>,
    head: Option<u32>,
    current: Option<u32>,
}

impl Registry {
    pub fn made(&mut self, object: u32) {
        assert!(!self.nodes.contains_key(&object));
        match self.head {
            Some(head) => {
                let next = self.nodes[&head].next;
                self.nodes.insert(object, Links { next, prev: head });
                self.nodes.get_mut(&head).unwrap().next = object;
                self.nodes.get_mut(&next).unwrap().prev = object;
            }
            None => {
                self.nodes.insert(object, Links { next: object, prev: object });
                self.head = Some(object);
            }
        }
        self.current = Some(object);
    }

    pub fn gone(&mut self, object: u32) {
        let Some(Links { next, prev }) = self.nodes.remove(&object) else { return };
        if self.nodes.is_empty() {
            self.head = None;
            self.current = None;
        } else {
            self.nodes.get_mut(&prev).unwrap().next = next;
            self.nodes.get_mut(&next).unwrap().prev = prev;
            self.head = Some(next);
            self.current = Some(next);
        }
    }

    pub fn current(&self) -> Option<u32> { self.current }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(registry: &mut Registry) -> Vec<u32> {
        let mut out = Vec::new();
        while let Some(object) = registry.current() {
            out.push(object);
            registry.gone(object);
        }
        out
    }

    #[test]
    fn release_starts_at_cursor_then_walks_the_ring_instead_of_reverse_creation_order() {
        let mut registry = Registry::default();
        for object in [1, 2, 3, 4] { registry.made(object); }
        assert_eq!(release(&mut registry), [4, 3, 2, 1]);
        for object in [1, 2, 3, 4] { registry.made(object); }
        registry.gone(2); // head/cursor move to 1, even though 2 wasn't head.
        assert_eq!(release(&mut registry), [1, 4, 3]);
    }

    #[test]
    fn insert_after_a_removal_uses_the_new_head_and_a_temporary_object_changes_the_cursor() {
        let mut registry = Registry::default();
        for object in [1, 2, 3] { registry.made(object); }
        registry.gone(3); // head/cursor = 2; ring 2,1.
        registry.made(4); // ring 2,4,1; cursor 4.
        registry.made(5); // temporary: ring 2,5,4,1.
        registry.gone(5); // head/cursor = 4; ring 4,1,2.
        assert_eq!(release(&mut registry), [4, 1, 2]);
        registry.made(6);
        registry.gone(6);
        assert_eq!(registry.current(), None);
    }
}
