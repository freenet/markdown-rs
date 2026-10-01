//! Deal with several changes in events, batching them together.
//!
//! Preferably, changes should be kept to a minimum.
//! Sometimes, it’s needed to change the list of events, because parsing can be
//! messy, and it helps to expose a cleaner interface of events to the compiler
//! and other users.
//! It can also help to merge many adjacent similar events.
//! And, in other cases, it’s needed to parse subcontent: pass some events
//! through another tokenizer and inject the result.

use crate::event::Event;
use alloc::{collections::BTreeMap, vec::Vec};

/// Shift `previous` and `next` links according to `jumps`.
///
/// This fixes links in case there are events removed or added between them.
fn shift_links(events: &mut [Event], jumps: &[(usize, usize, usize)]) {
    let mut jump_index = 0;
    let mut index = 0;
    let mut add = 0;
    let mut rm = 0;

    while index < events.len() {
        let rm_curr = rm;

        while jump_index < jumps.len() && jumps[jump_index].0 <= index {
            add = jumps[jump_index].2;
            rm = jumps[jump_index].1;
            jump_index += 1;
        }

        // Ignore items that will be removed.
        if rm > rm_curr {
            index += rm - rm_curr;
        } else {
            if let Some(link) = &events[index].link {
                if let Some(next) = link.next {
                    events[next].link.as_mut().unwrap().previous = Some(index + add - rm);

                    while jump_index < jumps.len() && jumps[jump_index].0 <= next {
                        add = jumps[jump_index].2;
                        rm = jumps[jump_index].1;
                        jump_index += 1;
                    }

                    events[index].link.as_mut().unwrap().next = Some(next + add - rm);
                    index = next;
                    continue;
                }
            }

            index += 1;
        }
    }
}

/// Tracks a bunch of edits.
///
/// Keyed by the index an edit applies at, so adding to an index that already
/// has an edit is a lookup rather than a scan of every edit so far (which
/// made a document with many edits quadratic to parse).
#[derive(Debug)]
pub struct EditMap {
    /// Record of changes: `at` -> (`remove`, `add`).
    map: BTreeMap<usize, (usize, Vec<Event>)>,
}

impl EditMap {
    /// Create a new edit map.
    pub fn new() -> EditMap {
        EditMap {
            map: BTreeMap::new(),
        }
    }
    /// Create an edit: a remove and/or add at a certain place.
    pub fn add(&mut self, index: usize, remove: usize, add: Vec<Event>) {
        add_impl(self, index, remove, add, false);
    }
    /// Create an edit: but insert `add` before existing additions.
    pub fn add_before(&mut self, index: usize, remove: usize, add: Vec<Event>) {
        add_impl(self, index, remove, add, true);
    }
    /// Done, change the events.
    pub fn consume(&mut self, events: &mut Vec<Event>) {
        if self.map.is_empty() {
            return;
        }

        // In order of `at`, as the map is ordered by its key.
        let mut map: Vec<(usize, usize, Vec<Event>)> = core::mem::take(&mut self.map)
            .into_iter()
            .map(|(at, (remove, add))| (at, remove, add))
            .collect();

        // Calculate jumps: where items in the current list move to.
        let mut jumps = Vec::with_capacity(map.len());
        let mut add_acc = 0;
        let mut remove_acc = 0;
        for (at, remove, add) in &map {
            remove_acc += remove;
            add_acc += add.len();
            jumps.push((*at, remove_acc, add_acc));
        }

        shift_links(events, &jumps);

        let len_before = events.len();
        let mut index = map.len();
        let mut vecs = Vec::with_capacity(index * 2 + 1);
        while index > 0 {
            index -= 1;
            vecs.push(events.split_off(map[index].0 + map[index].1));
            vecs.push(map[index].2.split_off(0));
            events.truncate(map[index].0);
        }
        vecs.push(events.split_off(0));

        events.reserve(len_before + add_acc - remove_acc);

        while let Some(mut slice) = vecs.pop() {
            events.append(&mut slice);
        }
    }
}

/// Create an edit.
fn add_impl(edit_map: &mut EditMap, at: usize, remove: usize, mut add: Vec<Event>, before: bool) {
    if remove == 0 && add.is_empty() {
        return;
    }

    if let Some(existing) = edit_map.map.get_mut(&at) {
        existing.0 += remove;

        if before {
            add.append(&mut existing.1);
            existing.1 = add;
        } else {
            existing.1.append(&mut add);
        }

        return;
    }

    edit_map.map.insert(at, (remove, add));
}
