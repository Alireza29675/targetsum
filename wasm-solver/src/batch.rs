use crate::solver::NumberEntry;

/// Iterative DFS state for resumable batch searching.
/// Converts the recursive branch-and-bound into an explicit stack so we can
/// pause after N nodes and yield control back to JS for progress updates.

/// One frame of the DFS stack — mirrors what the recursive version holds per call.
#[derive(Clone)]
struct Frame {
    start: usize,      // which sorted index to try next within this frame
    current_sum: u64,
    path_len: usize,   // how many elements in path when this frame was pushed
}

pub struct BatchSearchState {
    // Problem data (owned, lives for the duration of the search)
    sorted: Vec<NumberEntry>,
    suffix_sum: Vec<u64>,
    target: u64,
    min_count: usize,
    max_count: usize,
    max_results: usize,

    // DFS state
    stack: Vec<Frame>,
    path: Vec<usize>,       // indices into sorted[]
    results: Vec<Vec<NumberEntry>>,
    nodes_explored: u64,
    finished: bool,

    // For progress estimation: track how much of the top-level iteration we've done.
    // The top-level loop goes from 0..n, so top_level_index / n is a rough progress measure.
    top_level_n: usize,
    top_level_done: usize,
}

/// Result of one batch of work.
pub struct BatchResult {
    /// New combinations found in this batch
    pub new_results: Vec<Vec<NumberEntry>>,
    /// Total results found so far
    pub total_found: usize,
    /// Total DFS nodes explored so far
    pub nodes_explored: u64,
    /// Whether the search is completely finished
    pub finished: bool,
    /// Rough progress estimate 0.0 .. 1.0 (based on top-level iteration)
    pub progress: f64,
}

impl BatchSearchState {
    pub fn new(
        entries: &[NumberEntry],
        target: u64,
        min_count: usize,
        max_count: usize,
        max_results: usize,
    ) -> Self {
        let mut sorted: Vec<NumberEntry> = entries.to_vec();
        sorted.sort_unstable_by_key(|e| e.value);

        let n = sorted.len();
        let mut suffix_sum = vec![0u64; n + 1];
        for i in (0..n).rev() {
            suffix_sum[i] = suffix_sum[i + 1].saturating_add(sorted[i].value);
        }

        // Quick feasibility — if impossible, mark finished immediately
        let mut finished = false;
        if n == 0 || suffix_sum[0] < target || min_count > n {
            finished = true;
        }
        if !finished && min_count > 0 {
            let min_sum: u64 = sorted.iter().take(min_count).map(|e| e.value).sum();
            if min_sum > target {
                finished = true;
            }
        }

        // Seed the stack with the initial frame (start at index 0, sum 0, path empty)
        let mut stack = Vec::new();
        if !finished {
            stack.push(Frame {
                start: 0,
                current_sum: 0,
                path_len: 0,
            });
        }

        BatchSearchState {
            top_level_n: n,
            top_level_done: 0,
            sorted,
            suffix_sum,
            target,
            min_count,
            max_count,
            max_results,
            stack,
            path: Vec::new(),
            results: Vec::new(),
            nodes_explored: 0,
            finished,
        }
    }

    /// Run up to `node_budget` DFS nodes. Returns what was found in this batch.
    pub fn search_batch(&mut self, node_budget: u64) -> BatchResult {
        let prev_found = self.results.len();
        let mut budget = node_budget;

        while budget > 0 && !self.stack.is_empty() && self.results.len() < self.max_results {
            budget -= 1;
            self.nodes_explored += 1;

            let frame = self.stack.last_mut().unwrap();
            let current_sum = frame.current_sum;
            let path_len = frame.path_len;
            let start = frame.start;
            let n = self.sorted.len();

            // Trim path back to this frame's depth (backtrack)
            self.path.truncate(path_len);

            // Find next valid child to explore from `start`
            let remaining_budget_val = self.target - current_sum;
            let remaining_needed = self.min_count.saturating_sub(path_len);

            let mut found_child = false;
            let mut i = start;

            while i < n {
                let value = self.sorted[i].value;

                // Pruning: element too large
                if value > remaining_budget_val {
                    break;
                }
                // Pruning: suffix sum insufficient
                if self.suffix_sum[i] < remaining_budget_val {
                    break;
                }
                // Pruning: not enough elements left for min_count
                if (n - i) < remaining_needed {
                    break;
                }

                // This child is worth exploring. Advance frame.start past it
                // so when we pop back, we try the next sibling.
                let frame = self.stack.last_mut().unwrap();
                frame.start = i + 1;

                let new_sum = current_sum + value;
                let new_path_len = path_len + 1;

                // Push this element onto path
                self.path.truncate(path_len);
                self.path.push(i);

                // Track top-level progress
                if path_len == 0 {
                    self.top_level_done = i + 1;
                }

                // Check if this is a solution
                if new_sum == self.target && new_path_len >= self.min_count {
                    let combo: Vec<NumberEntry> = self.path.iter()
                        .map(|&idx| self.sorted[idx].clone())
                        .collect();
                    self.results.push(combo);
                    if self.results.len() >= self.max_results {
                        // Drain the stack — we're done
                        self.stack.clear();
                        self.finished = true;
                        break;
                    }
                    // With positive integers, can't extend this path further.
                    // Continue to next sibling (frame.start already advanced).
                    i += 1;
                    continue;
                }

                // If we can go deeper, push a new frame for the child
                if new_path_len < self.max_count {
                    self.stack.push(Frame {
                        start: i + 1,
                        current_sum: new_sum,
                        path_len: new_path_len,
                    });
                }

                found_child = true;
                break;
            }

            if !found_child || i >= n || self.sorted[i].value > remaining_budget_val {
                // No more children in this frame — pop it
                // But only if we didn't just push a new child frame
                if !found_child {
                    self.stack.pop();
                    // Update top-level progress when a top-level branch is exhausted
                    if self.stack.len() <= 1 && path_len <= 1 {
                        self.top_level_done = start;
                    }
                }
            }
        }

        if self.stack.is_empty() || self.results.len() >= self.max_results {
            self.finished = true;
        }

        let new_results: Vec<Vec<NumberEntry>> = self.results[prev_found..].to_vec();

        let progress = if self.top_level_n > 0 {
            (self.top_level_done as f64) / (self.top_level_n as f64)
        } else {
            1.0
        };

        BatchResult {
            new_results,
            total_found: self.results.len(),
            nodes_explored: self.nodes_explored,
            finished: self.finished,
            progress: if self.finished { 1.0 } else { progress.min(0.999) },
        }
    }

    /// Get all results found so far.
    #[allow(dead_code)]
    pub fn all_results(&self) -> &[Vec<NumberEntry>] {
        &self.results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entries(nums: &[u64]) -> Vec<NumberEntry> {
        nums.iter().enumerate()
            .map(|(i, &v)| NumberEntry { value: v, original_index: i })
            .collect()
    }

    #[test]
    fn test_batch_finds_all() {
        let entries = make_entries(&[1, 2, 3, 4, 5]);
        let mut state = BatchSearchState::new(&entries, 5, 1, 5, 100);

        // Run in small batches
        loop {
            let result = state.search_batch(10);
            if result.finished {
                assert_eq!(result.total_found, 3); // [5], [1,4], [2,3]
                break;
            }
        }
    }

    #[test]
    fn test_batch_respects_max_results() {
        let entries = make_entries(&[1, 2, 3, 4, 5]);
        let mut state = BatchSearchState::new(&entries, 5, 1, 5, 2);

        loop {
            let result = state.search_batch(100);
            if result.finished {
                assert!(result.total_found <= 2);
                break;
            }
        }
    }

    #[test]
    fn test_batch_progress_increases() {
        let entries = make_entries(&(1..=20).collect::<Vec<u64>>());
        let mut state = BatchSearchState::new(&entries, 30, 2, 5, 1000);

        let mut last_progress = -1.0f64;
        loop {
            let result = state.search_batch(50);
            assert!(result.progress >= last_progress || result.finished);
            last_progress = result.progress;
            if result.finished {
                break;
            }
        }
        assert_eq!(last_progress, 1.0);
    }

    #[test]
    fn test_batch_empty_input() {
        let entries: Vec<NumberEntry> = vec![];
        let mut state = BatchSearchState::new(&entries, 10, 1, 5, 100);
        let result = state.search_batch(100);
        assert!(result.finished);
        assert_eq!(result.total_found, 0);
    }

    #[test]
    fn test_batch_no_solution() {
        let entries = make_entries(&[10, 20, 30]);
        let mut state = BatchSearchState::new(&entries, 5, 1, 3, 100);
        let result = state.search_batch(10000);
        assert!(result.finished);
        assert_eq!(result.total_found, 0);
    }

    #[test]
    fn test_batch_large_numbers() {
        let entries = make_entries(&[
            500_000_000_000u64,
            250_000_000_000,
            125_000_000_000,
            375_000_000_000,
        ]);
        let mut state = BatchSearchState::new(&entries, 875_000_000_000, 2, 4, 100);
        loop {
            let result = state.search_batch(1000);
            if result.finished {
                assert!(result.total_found >= 1);
                break;
            }
        }
    }
}
