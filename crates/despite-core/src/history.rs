use std::collections::HashSet;
use std::sync::Arc;

use crate::project::Project;

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub Label: String,
    pub State: Project,
}

#[derive(Clone, Debug)]
pub struct History {
    pub Undo: Vec<HistoryEntry>,
    pub Redo: Vec<HistoryEntry>,
    pub Limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(100)
    }
}

impl History {
    pub fn new(Limit: usize) -> Self {
        Self { Undo: Vec::new(), Redo: Vec::new(), Limit: Limit.max(1) }
    }

    pub fn record(&mut self, Label: impl Into<String>, Current: &Project) {
        self.Undo.push(HistoryEntry { Label: Label.into(), State: Current.clone() });
        self.Redo.clear();
        self.trim();
    }

    pub fn trim(&mut self) {
        if self.Undo.len() > self.Limit {
            let Excess = self.Undo.len() - self.Limit;
            self.Undo.drain(0..Excess);
        }
    }

    pub fn set_limit(&mut self, Limit: usize) {
        self.Limit = Limit.max(1);
        self.trim();
    }

    pub fn undo(&mut self, Current: &mut Project) -> Option<String> {
        let Entry = self.Undo.pop()?;
        let Previous = std::mem::replace(Current, Entry.State);
        self.Redo.push(HistoryEntry { Label: Entry.Label.clone(), State: Previous });
        Some(Entry.Label)
    }

    pub fn redo(&mut self, Current: &mut Project) -> Option<String> {
        let Entry = self.Redo.pop()?;
        let Previous = std::mem::replace(Current, Entry.State);
        self.Undo.push(HistoryEntry { Label: Entry.Label.clone(), State: Previous });
        Some(Entry.Label)
    }

    pub fn can_undo(&self) -> bool {
        !self.Undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.Redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.Undo.last().map(|Entry| Entry.Label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.Redo.last().map(|Entry| Entry.Label.as_str())
    }

    pub fn clear(&mut self) {
        self.Undo.clear();
        self.Redo.clear();
    }

    pub fn len(&self) -> usize {
        self.Undo.len() + self.Redo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn extra_memory_bytes(&self, Current: &Project) -> usize {
        let mut Seen: HashSet<*const crate::raster::Raster> = HashSet::new();
        for Found in &Current.Images {
            for Entry in &Found.Layers {
                Seen.insert(Arc::as_ptr(&Entry.Pixels));
            }
        }
        let mut Total = 0usize;
        for Snapshot in self.Undo.iter().chain(self.Redo.iter()) {
            for Found in &Snapshot.State.Images {
                for Entry in &Found.Layers {
                    if Seen.insert(Arc::as_ptr(&Entry.Pixels)) {
                        Total += Entry.Pixels.byte_size();
                    }
                }
            }
        }
        Total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_roundtrip_shares_pixels() {
        let mut Current = Project::with_image("P", 4, 4);
        let mut Log = History::new(10);
        Log.record("Paint", &Current);
        Current.Images[0].Layers[0].pixels_mut().put(0, 0, [255, 0, 0, 255]);
        assert_eq!(Log.extra_memory_bytes(&Current), 64);
        assert_eq!(Log.undo(&mut Current).as_deref(), Some("Paint"));
        assert_eq!(Current.Images[0].Layers[0].Pixels.get(0, 0), [0, 0, 0, 0]);
        assert_eq!(Log.redo(&mut Current).as_deref(), Some("Paint"));
        assert_eq!(Current.Images[0].Layers[0].Pixels.get(0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn limit_trims_oldest() {
        let Current = Project::with_image("P", 1, 1);
        let mut Log = History::new(3);
        for Index in 0..5 {
            Log.record(format!("Step {Index}"), &Current);
        }
        assert_eq!(Log.Undo.len(), 3);
        assert_eq!(Log.Undo[0].Label, "Step 2");
    }
}
