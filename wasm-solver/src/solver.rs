use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;

/// A number with its original position in the CSV input.
#[derive(Clone, Debug)]
pub struct NumberEntry {
    pub value: u64,
    pub original_index: usize,
}

pub struct SolverConfig<'a> {
    pub target: u64,
    pub min_count: usize,
    pub max_count: usize,
    pub cancelled: &'a AtomicBool,
}

pub enum SolverResult {
    Found(Vec<NumberEntry>),
    NotFound,
    Cancelled,
}

/// Preprocessed data: sorted entries plus suffix sums for pruning.
/// No filtering here — input is already filtered by the caller (lib.rs).
struct PreparedData {
    /// Entries sorted by value ascending, with original indices preserved
    sorted: Vec<NumberEntry>,
    /// suffix_sum[i] = sum of sorted[i..].value
    suffix_sum: Vec<u64>,
}

impl PreparedData {
    fn new(entries: &[NumberEntry]) -> Self {
        let mut sorted: Vec<NumberEntry> = entries.to_vec();
        sorted.sort_unstable_by_key(|e| e.value);

        let n = sorted.len();
        let mut suffix_sum = vec![0u64; n + 1];
        for i in (0..n).rev() {
            suffix_sum[i] = suffix_sum[i + 1].saturating_add(sorted[i].value);
        }

        PreparedData { sorted, suffix_sum }
    }
}

/// Find ONE valid subset summing to target with count in [min_count, max_count].
///
/// Strategy:
/// - n <= 40: meet-in-the-middle (handles any target size, exhaustive for small n)
/// - n > 40: branch-and-bound DFS with aggressive pruning
pub fn solve_subset_sum(entries: &[NumberEntry], config: &SolverConfig) -> SolverResult {
    let data = PreparedData::new(entries);
    let n = data.sorted.len();

    if n == 0 {
        return SolverResult::NotFound;
    }

    // Quick feasibility checks
    if data.suffix_sum[0] < config.target {
        return SolverResult::NotFound;
    }
    if config.min_count > n || config.max_count < 1 {
        return SolverResult::NotFound;
    }
    if config.min_count > 0 {
        let min_sum: u64 = data.sorted.iter().take(config.min_count).map(|e| e.value).sum();
        if min_sum > config.target {
            return SolverResult::NotFound;
        }
    }

    if n <= 40 && config.max_count >= config.min_count {
        if let Some(result) = meet_in_the_middle(&data, config) {
            return SolverResult::Found(result);
        }
        if config.cancelled.load(Ordering::Relaxed) {
            return SolverResult::Cancelled;
        }
        return SolverResult::NotFound;
    }

    branch_and_bound_first(&data, config)
}

/// Find ALL valid combinations (up to max_results).
/// Used by tests; the WASM API uses BatchSearchState for streaming instead.
#[allow(dead_code)]
pub fn solve_all_combinations(
    entries: &[NumberEntry],
    config: &SolverConfig,
    max_results: usize,
) -> Vec<Vec<NumberEntry>> {
    let data = PreparedData::new(entries);
    let n = data.sorted.len();

    if n == 0 || data.suffix_sum[0] < config.target || config.min_count > n {
        return Vec::new();
    }
    if config.min_count > 0 {
        let min_sum: u64 = data.sorted.iter().take(config.min_count).map(|e| e.value).sum();
        if min_sum > config.target {
            return Vec::new();
        }
    }

    let mut results = Vec::new();
    let mut path = Vec::new();
    let mut check_counter = 0u64;
    branch_and_bound_all(
        &data, config, 0, 0, &mut path, &mut results, max_results, &mut check_counter,
    );
    results
}

// ---------------------------------------------------------------------------
// Meet-in-the-middle: split into two halves, enumerate all subsets per half,
// find complementary pairs via hash map.
// Time: O(2^(n/2)), Space: O(2^(n/2)). Works for n up to ~40.
// ---------------------------------------------------------------------------

fn meet_in_the_middle(data: &PreparedData, config: &SolverConfig) -> Option<Vec<NumberEntry>> {
    let n = data.sorted.len();
    let mid = n / 2;
    let left = &data.sorted[..mid];
    let right = &data.sorted[mid..];

    let left_len = left.len();
    let right_len = right.len();
    let left_count = 1u64 << left_len;

    // sum -> Vec<(count, bitmask)>
    let mut left_map: HashMap<u64, Vec<(usize, u64)>> = HashMap::with_capacity(left_count as usize);

    for mask in 0..left_count {
        if mask & 0xFFFF == 0 && config.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let mut sum = 0u64;
        let mut count = 0usize;
        let mut overflow = false;
        for bit in 0..left_len {
            if mask & (1u64 << bit) != 0 {
                sum += left[bit].value;
                count += 1;
                if sum > config.target {
                    overflow = true;
                    break;
                }
            }
        }
        if overflow { continue; }
        if count <= config.max_count {
            left_map.entry(sum).or_default().push((count, mask));
        }
    }

    let right_count = 1u64 << right_len;

    for rmask in 0..right_count {
        if rmask & 0xFFFF == 0 && config.cancelled.load(Ordering::Relaxed) {
            return None;
        }
        let mut rsum = 0u64;
        let mut rcount = 0usize;
        let mut overflow = false;
        for bit in 0..right_len {
            if rmask & (1u64 << bit) != 0 {
                rsum += right[bit].value;
                rcount += 1;
                if rsum > config.target {
                    overflow = true;
                    break;
                }
            }
        }
        if overflow { continue; }

        let needed = config.target - rsum;
        if let Some(left_entries) = left_map.get(&needed) {
            for &(lcount, lmask) in left_entries {
                let total_count = lcount + rcount;
                if total_count >= config.min_count && total_count <= config.max_count {
                    let mut result = Vec::with_capacity(total_count);
                    for bit in 0..left_len {
                        if lmask & (1u64 << bit) != 0 {
                            result.push(left[bit].clone());
                        }
                    }
                    for bit in 0..right_len {
                        if rmask & (1u64 << bit) != 0 {
                            result.push(right[bit].clone());
                        }
                    }
                    result.sort_unstable_by_key(|e| e.original_index);
                    return Some(result);
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Branch-and-bound DFS with aggressive pruning.
// ---------------------------------------------------------------------------

fn branch_and_bound_first(data: &PreparedData, config: &SolverConfig) -> SolverResult {
    let mut path: Vec<usize> = Vec::with_capacity(config.max_count);
    let mut check_counter = 0u64;

    match bb_dfs_first(data, config, 0, 0, 0, &mut path, &mut check_counter) {
        BbResult::Found => {
            let entries: Vec<NumberEntry> = path.iter()
                .map(|&i| data.sorted[i].clone())
                .collect();
            SolverResult::Found(entries)
        }
        BbResult::Cancelled => SolverResult::Cancelled,
        BbResult::NotFound => SolverResult::NotFound,
    }
}

enum BbResult {
    Found,
    NotFound,
    Cancelled,
}

fn bb_dfs_first(
    data: &PreparedData,
    config: &SolverConfig,
    start: usize,
    current_sum: u64,
    current_count: usize,
    path: &mut Vec<usize>,
    check_counter: &mut u64,
) -> BbResult {
    // Check cancellation every 4096 nodes (amortized cost of atomic load)
    *check_counter = check_counter.wrapping_add(1);
    if *check_counter & 0xFFF == 0 && config.cancelled.load(Ordering::Relaxed) {
        return BbResult::Cancelled;
    }

    if current_sum == config.target && current_count >= config.min_count {
        return BbResult::Found;
    }

    if current_count >= config.max_count {
        return BbResult::NotFound;
    }

    let n = data.sorted.len();
    let remaining_needed = config.min_count.saturating_sub(current_count);

    if n - start < remaining_needed {
        return BbResult::NotFound;
    }

    let remaining_budget = config.target - current_sum;

    for i in start..n {
        let value = data.sorted[i].value;

        // Since sorted ascending, once one element exceeds budget, all after do too
        if value > remaining_budget {
            break;
        }

        // If sum of all remaining elements can't reach target, prune
        if data.suffix_sum[i] < remaining_budget {
            break;
        }

        // Not enough elements left to meet min_count
        if (n - i) < remaining_needed {
            break;
        }

        path.push(i);
        let result = bb_dfs_first(
            data, config, i + 1,
            current_sum + value,
            current_count + 1,
            path, check_counter,
        );

        match result {
            BbResult::Found => return BbResult::Found,
            BbResult::Cancelled => return BbResult::Cancelled,
            BbResult::NotFound => { path.pop(); }
        }
    }

    BbResult::NotFound
}

#[allow(dead_code)]
fn branch_and_bound_all(
    data: &PreparedData,
    config: &SolverConfig,
    start: usize,
    current_sum: u64,
    path: &mut Vec<usize>,
    results: &mut Vec<Vec<NumberEntry>>,
    max_results: usize,
    check_counter: &mut u64,
) {
    let current_count = path.len();

    // Periodic cancellation check (every 4096 nodes)
    *check_counter = check_counter.wrapping_add(1);
    if *check_counter & 0xFFF == 0 && config.cancelled.load(Ordering::Relaxed) {
        return;
    }

    // Found a valid solution
    if current_sum == config.target && current_count >= config.min_count {
        let entries: Vec<NumberEntry> = path.iter()
            .map(|&i| data.sorted[i].clone())
            .collect();
        results.push(entries);
        if results.len() >= max_results {
            return;
        }
        // All values are positive, so adding more elements would exceed target.
        return;
    }

    if current_count >= config.max_count || results.len() >= max_results {
        return;
    }

    let n = data.sorted.len();
    let remaining_needed = config.min_count.saturating_sub(current_count);

    if n - start < remaining_needed {
        return;
    }

    let remaining_budget = config.target - current_sum;

    for i in start..n {
        let value = data.sorted[i].value;

        if value > remaining_budget {
            break;
        }
        if data.suffix_sum[i] < remaining_budget {
            break;
        }
        if (n - i) < remaining_needed {
            break;
        }
        if results.len() >= max_results {
            return;
        }

        path.push(i);
        branch_and_bound_all(
            data, config, i + 1,
            current_sum + value,
            path, results, max_results, check_counter,
        );
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn make_entries(nums: &[u64]) -> Vec<NumberEntry> {
        nums.iter().enumerate()
            .map(|(i, &v)| NumberEntry { value: v, original_index: i })
            .collect()
    }

    fn make_config(target: u64, min: usize, max: usize) -> SolverConfig<'static> {
        static FALSE: AtomicBool = AtomicBool::new(false);
        SolverConfig {
            target,
            min_count: min,
            max_count: max,
            cancelled: &FALSE,
        }
    }

    #[test]
    fn test_simple_case() {
        let nums = vec![1, 2, 3, 4, 5];
        let entries = make_entries(&nums);
        let config = make_config(9, 2, 3);
        match solve_subset_sum(&entries, &config) {
            SolverResult::Found(result) => {
                let sum: u64 = result.iter().map(|e| e.value).sum();
                assert_eq!(sum, 9);
                assert!(result.len() >= 2 && result.len() <= 3);
                // Verify original indices are correct
                for e in &result {
                    assert_eq!(e.value, nums[e.original_index]);
                }
            }
            _ => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_no_solution() {
        let nums = vec![10, 20, 30];
        let entries = make_entries(&nums);
        let config = make_config(5, 1, 3);
        match solve_subset_sum(&entries, &config) {
            SolverResult::NotFound => {}
            _ => panic!("Should not have found a solution"),
        }
    }

    #[test]
    fn test_exact_single() {
        let nums = vec![100, 200, 300];
        let entries = make_entries(&nums);
        let config = make_config(200, 1, 1);
        match solve_subset_sum(&entries, &config) {
            SolverResult::Found(result) => {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].value, 200);
                assert_eq!(result[0].original_index, 1);
            }
            _ => panic!("Should have found 200"),
        }
    }

    #[test]
    fn test_all_combinations() {
        let nums = vec![1, 2, 3, 4, 5];
        let entries = make_entries(&nums);
        let config = make_config(5, 1, 5);
        let results = solve_all_combinations(&entries, &config, 100);
        // Valid combos: [5], [1,4], [2,3]
        assert!(results.len() >= 3);
        for combo in &results {
            let sum: u64 = combo.iter().map(|e| e.value).sum();
            assert_eq!(sum, 5);
        }
    }

    #[test]
    fn test_large_numbers() {
        let nums = vec![
            1_000_000_000_000u64,
            500_000_000_000,
            250_000_000_000,
            125_000_000_000,
            375_000_000_000,
        ];
        let entries = make_entries(&nums);
        let config = make_config(875_000_000_000, 2, 4);
        match solve_subset_sum(&entries, &config) {
            SolverResult::Found(result) => {
                let sum: u64 = result.iter().map(|e| e.value).sum();
                assert_eq!(sum, 875_000_000_000);
                for e in &result {
                    assert_eq!(e.value, nums[e.original_index]);
                }
            }
            _ => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_meet_in_the_middle_threshold() {
        let nums: Vec<u64> = (1..=30).collect();
        let entries = make_entries(&nums);
        let config = make_config(100, 3, 10);
        match solve_subset_sum(&entries, &config) {
            SolverResult::Found(result) => {
                let sum: u64 = result.iter().map(|e| e.value).sum();
                assert_eq!(sum, 100);
                for e in &result {
                    assert_eq!(e.value, nums[e.original_index]);
                }
            }
            _ => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_original_indices_preserved_with_gaps() {
        // Simulate CSV with some invalid rows filtered by lib.rs
        // Original CSV: [0, 5, 0, 3, 7, 0, 2] -> after filtering: entries at indices 1,3,4,6
        let entries = vec![
            NumberEntry { value: 5, original_index: 1 },
            NumberEntry { value: 3, original_index: 3 },
            NumberEntry { value: 7, original_index: 4 },
            NumberEntry { value: 2, original_index: 6 },
        ];
        let config = make_config(10, 2, 4);
        match solve_subset_sum(&entries, &config) {
            SolverResult::Found(result) => {
                let sum: u64 = result.iter().map(|e| e.value).sum();
                assert_eq!(sum, 10);
                // Verify original indices are from the original set {1,3,4,6}
                for e in &result {
                    assert!(
                        [1, 3, 4, 6].contains(&e.original_index),
                        "Unexpected original_index: {}",
                        e.original_index
                    );
                }
            }
            _ => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_branch_and_bound_large_n() {
        // 50 elements - should use B&B, not MITM
        let nums: Vec<u64> = (1..=50).collect();
        let entries = make_entries(&nums);
        let config = make_config(50, 1, 5);
        match solve_subset_sum(&entries, &config) {
            SolverResult::Found(result) => {
                let sum: u64 = result.iter().map(|e| e.value).sum();
                assert_eq!(sum, 50);
            }
            _ => panic!("Should have found a solution"),
        }
    }
}
