use super::types::RepeatMode;

/// In-memory playlist manager with shuffle, repeat, and back/forward.\n/// Not yet wired into the daemon — planned for future playlist daemon integration.
#[allow(dead_code)]
pub struct QueueManager {
    pub index: usize,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub shuffle_order: Vec<usize>,
}

#[allow(dead_code)]
impl QueueManager {
    pub fn new() -> Self {
        Self {
            index: 0,
            repeat: RepeatMode::Off,
            shuffle: false,
            shuffle_order: Vec::new(),
        }
    }

    /// Returns the actual queue index accounting for shuffle order
    pub fn actual_index(&self, queue_len: usize) -> usize {
        if queue_len == 0 {
            return 0;
        }
        if self.shuffle && !self.shuffle_order.is_empty() {
            self.shuffle_order
                .get(self.index)
                .copied()
                .unwrap_or(self.index)
        } else {
            self.index.min(queue_len.saturating_sub(1))
        }
    }

    /// Advance to next track. Returns `Some(actual_index)` or `None` if queue ended.
    pub fn advance(&mut self, queue_len: usize) -> Option<usize> {
        if queue_len == 0 {
            return None;
        }

        match self.repeat {
            RepeatMode::One => Some(self.actual_index(queue_len)),
            _ => {
                let next = self.index + 1;
                if next >= queue_len {
                    match self.repeat {
                        RepeatMode::All => {
                            self.index = 0;
                            if self.shuffle {
                                self.reshuffle(queue_len);
                            }
                            Some(self.actual_index(queue_len))
                        }
                        _ => None,
                    }
                } else {
                    self.index = next;
                    Some(self.actual_index(queue_len))
                }
            }
        }
    }

    /// Go back. Returns `(actual_index, should_restart_current_track)`.
    ///
    /// If past threshold (>3s into track), restart current track.
    /// Otherwise go to previous track.
    pub fn go_back(&mut self, queue_len: usize, past_threshold: bool) -> (usize, bool) {
        if past_threshold {
            (self.actual_index(queue_len), true)
        } else if self.index > 0 {
            self.index -= 1;
            (self.actual_index(queue_len), false)
        } else {
            (self.actual_index(queue_len), true)
        }
    }

    /// Fisher-Yates shuffle using fastrand
    pub fn reshuffle(&mut self, queue_len: usize) {
        self.shuffle_order = (0..queue_len).collect();
        for i in (1..queue_len).rev() {
            let j = fastrand::usize(..=i);
            self.shuffle_order.swap(i, j);
        }
    }

    pub fn toggle_repeat(&mut self) -> RepeatMode {
        self.repeat = self.repeat.cycle();
        self.repeat
    }

    #[allow(dead_code)]
    pub fn toggle_shuffle(&mut self, queue_len: usize) -> bool {
        self.shuffle = !self.shuffle;
        if self.shuffle {
            self.reshuffle(queue_len);
        }
        self.shuffle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_sequential() {
        let mut qm = QueueManager::new();
        assert_eq!(qm.advance(5), Some(1));
        assert_eq!(qm.advance(5), Some(2));
        assert_eq!(qm.index, 2);
    }

    #[test]
    fn advance_repeat_off_ends() {
        let mut qm = QueueManager::new();
        qm.index = 4;
        assert_eq!(qm.advance(5), None);
    }

    #[test]
    fn advance_repeat_all_wraps() {
        let mut qm = QueueManager::new();
        qm.repeat = RepeatMode::All;
        qm.index = 4;
        assert_eq!(qm.advance(5), Some(0));
        assert_eq!(qm.index, 0);
    }

    #[test]
    fn advance_repeat_one_stays() {
        let mut qm = QueueManager::new();
        qm.repeat = RepeatMode::One;
        qm.index = 2;
        assert_eq!(qm.advance(5), Some(2));
        assert_eq!(qm.index, 2);
    }

    #[test]
    fn go_back_restarts_past_threshold() {
        let mut qm = QueueManager::new();
        qm.index = 3;
        let (idx, restart) = qm.go_back(5, true);
        assert_eq!(idx, 3);
        assert!(restart);
    }

    #[test]
    fn go_back_goes_previous() {
        let mut qm = QueueManager::new();
        qm.index = 3;
        let (idx, restart) = qm.go_back(5, false);
        assert_eq!(idx, 2);
        assert!(!restart);
    }

    #[test]
    fn go_back_at_start_restarts() {
        let mut qm = QueueManager::new();
        let (idx, restart) = qm.go_back(5, false);
        assert_eq!(idx, 0);
        assert!(restart);
    }

    #[test]
    fn shuffle_no_duplicates() {
        let mut qm = QueueManager::new();
        qm.reshuffle(10);
        let mut sorted = qm.shuffle_order.clone();
        sorted.sort();
        assert_eq!(sorted, (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn toggle_repeat_cycles() {
        let mut qm = QueueManager::new();
        assert_eq!(qm.toggle_repeat(), RepeatMode::One);
        assert_eq!(qm.toggle_repeat(), RepeatMode::All);
        assert_eq!(qm.toggle_repeat(), RepeatMode::Off);
    }

    #[test]
    fn advance_empty_queue() {
        let mut qm = QueueManager::new();
        assert_eq!(qm.advance(0), None);
    }
}
