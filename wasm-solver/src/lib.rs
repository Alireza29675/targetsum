mod utils;
mod solver;
mod batch;

use wasm_bindgen::prelude::*;
use solver::{SolverConfig, SolverResult, NumberEntry, solve_subset_sum};
use batch::BatchSearchState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::cell::RefCell;

static CANCELLED: AtomicBool = AtomicBool::new(false);

// Hold the batch search state across calls in thread-local storage.
// WASM is single-threaded so this is safe.
thread_local! {
    static BATCH_STATE: RefCell<Option<BatchSearchState>> = RefCell::new(None);
}

#[wasm_bindgen]
pub fn init_panic_hook() {
    utils::set_panic_hook();
}

/// Cancel a running computation.
#[wasm_bindgen]
pub fn cancel_search() {
    CANCELLED.store(true, Ordering::Relaxed);
}

/// Build input entries, preserving original CSV row indices.
fn build_entries(numbers: &[f64], target: u64) -> Vec<NumberEntry> {
    numbers.iter()
        .enumerate()
        .filter_map(|(original_idx, &n)| {
            let v = n as u64;
            if v > 0 && v <= target {
                Some(NumberEntry { value: v, original_index: original_idx })
            } else {
                None
            }
        })
        .collect()
}

/// Find ONE valid combination. Returns a JSON string.
#[wasm_bindgen]
pub fn find_one(
    numbers: &[f64],
    target: f64,
    min_count: u32,
    max_count: u32,
) -> String {
    CANCELLED.store(false, Ordering::Relaxed);

    let target = target as u64;
    let entries = build_entries(numbers, target);

    let config = SolverConfig {
        target,
        min_count: min_count as usize,
        max_count: max_count as usize,
        cancelled: &CANCELLED,
    };

    let result = solve_subset_sum(&entries, &config);
    result_to_json(&result)
}

/// Initialize a batch search for ALL combinations.
/// Call search_batch() repeatedly until it returns finished=true.
#[wasm_bindgen]
pub fn init_batch_search(
    numbers: &[f64],
    target: f64,
    min_count: u32,
    max_count: u32,
    max_results: u32,
) {
    let target = target as u64;
    let entries = build_entries(numbers, target);

    let state = BatchSearchState::new(
        &entries,
        target,
        min_count as usize,
        max_count as usize,
        max_results as usize,
    );

    BATCH_STATE.with(|cell| {
        *cell.borrow_mut() = Some(state);
    });
}

/// Run one batch of DFS work (node_budget nodes).
/// Returns JSON: { new_results: [...], total_found, nodes_explored, finished, progress }
#[wasm_bindgen]
pub fn search_batch(node_budget: u32) -> String {
    BATCH_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        match borrow.as_mut() {
            None => r#"{"error":"no search initialized"}"#.to_string(),
            Some(state) => {
                let result = state.search_batch(node_budget as u64);
                batch_result_to_json(&result)
            }
        }
    })
}

/// Clean up batch search state to free memory.
#[wasm_bindgen]
pub fn destroy_batch_search() {
    BATCH_STATE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

fn result_to_json(result: &SolverResult) -> String {
    match result {
        SolverResult::Found(entries) => {
            let indices_str: Vec<String> = entries.iter().map(|e| e.original_index.to_string()).collect();
            let values_str: Vec<String> = entries.iter().map(|e| e.value.to_string()).collect();
            format!(
                r#"{{"status":"found","indices":[{}],"values":[{}],"count":{}}}"#,
                indices_str.join(","),
                values_str.join(","),
                entries.len()
            )
        }
        SolverResult::NotFound => r#"{"status":"not_found"}"#.to_string(),
        SolverResult::Cancelled => r#"{"status":"cancelled"}"#.to_string(),
    }
}

fn entries_to_json(entries: &[NumberEntry]) -> String {
    let indices_str: Vec<String> = entries.iter().map(|e| e.original_index.to_string()).collect();
    let values_str: Vec<String> = entries.iter().map(|e| e.value.to_string()).collect();
    format!(
        r#"{{"indices":[{}],"values":[{}],"count":{}}}"#,
        indices_str.join(","),
        values_str.join(","),
        entries.len()
    )
}

fn batch_result_to_json(result: &batch::BatchResult) -> String {
    let new_combos: Vec<String> = result.new_results.iter()
        .map(|entries| entries_to_json(entries))
        .collect();

    format!(
        r#"{{"new_results":[{}],"total_found":{},"nodes_explored":{},"finished":{},"progress":{:.6}}}"#,
        new_combos.join(","),
        result.total_found,
        result.nodes_explored,
        result.finished,
        result.progress,
    )
}
