/* tslint:disable */
/* eslint-disable */

/**
 * Cancel a running computation.
 */
export function cancel_search(): void;

/**
 * Clean up batch search state to free memory.
 */
export function destroy_batch_search(): void;

/**
 * Find ONE valid combination. Returns a JSON string.
 */
export function find_one(numbers: Float64Array, target: number, min_count: number, max_count: number): string;

/**
 * Initialize a batch search for ALL combinations.
 * Call search_batch() repeatedly until it returns finished=true.
 */
export function init_batch_search(numbers: Float64Array, target: number, min_count: number, max_count: number, max_results: number): void;

export function init_panic_hook(): void;

/**
 * Run one batch of DFS work (node_budget nodes).
 * Returns JSON: { new_results: [...], total_found, nodes_explored, finished, progress }
 */
export function search_batch(node_budget: number): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly find_one: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly init_batch_search: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly search_batch: (a: number) => [number, number];
    readonly init_panic_hook: () => void;
    readonly destroy_batch_search: () => void;
    readonly cancel_search: () => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
