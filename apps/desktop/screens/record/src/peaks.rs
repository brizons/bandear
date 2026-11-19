//! The peaks a waveform lane is drawn from (`createPeakHistory`, `record-view.js`).

/// A bounded history of peak levels for one track's waveform lane: [PeakHistory::capacity] levels
/// at most, the oldest dropped as a new one comes, kept in a fixed buffer, so it never grows
/// however long a take runs.
#[derive(Clone, Debug)]
pub struct PeakHistory {
    levels: Vec<f32>,
    start: usize,
    length: usize,
}

impl PeakHistory {
    /// A history of `capacity` levels (at least one).
    pub fn new(capacity: usize) -> Self {
        Self {
            levels: vec![0.0; capacity.max(1)],
            start: 0,
            length: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.levels.len()
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn push(&mut self, level: f32) {
        let size = self.levels.len();
        if self.length < size {
            self.levels[(self.start + self.length) % size] = level;
            self.length += 1;
        } else {
            self.levels[self.start] = level;
            self.start = (self.start + 1) % size;
        }
    }

    /// The level `index` from the oldest.
    pub fn at(&self, index: usize) -> f32 {
        self.levels[(self.start + index) % self.levels.len()]
    }

    /// The levels from the oldest.
    pub fn iter(&self) -> impl Iterator<Item = f32> + '_ {
        (0..self.length).map(|index| self.at(index))
    }

    pub fn clear(&mut self) {
        self.start = 0;
        self.length = 0;
    }
}
