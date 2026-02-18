// Web Worker: runs WASM computation off the main thread.
// For "find all", uses batch API so we can yield progress to the main thread.
//
// Protocol:
//   Main → Worker: { type: "find_one"|"find_all", ...params }
//   Worker → Main: { type: "result"|"progress"|"error"|"ready", ...data }
// Cancel is handled by main thread terminating + recreating the worker.

import init, {
  find_one,
  init_batch_search,
  search_batch,
  destroy_batch_search,
  init_panic_hook,
} from './wasm-solver/pkg/wasm_solver.js';

let wasmReady = false;

async function initWasm() {
  try {
    await init();
    init_panic_hook();
    wasmReady = true;
    self.postMessage({ type: 'ready' });
  } catch (e) {
    self.postMessage({ type: 'error', message: 'Failed to initialize WASM: ' + e.message });
  }
}

initWasm();

// Nodes to process per batch. Tuned so each batch takes ~20-50ms,
// giving smooth progress updates without too much overhead.
const NODES_PER_BATCH = 200_000;

self.onmessage = function(e) {
  const msg = e.data;

  if (!wasmReady) {
    self.postMessage({ type: 'error', message: 'WASM not ready yet' });
    return;
  }

  if (msg.type === 'find_one') {
    try {
      const numbers = new Float64Array(msg.numbers);
      const t0 = performance.now();
      const resultJson = find_one(numbers, msg.target, msg.minCount, msg.maxCount);
      const elapsed = performance.now() - t0;
      const result = JSON.parse(resultJson);
      result.elapsed_ms = elapsed;
      self.postMessage({ type: 'result', data: result });
    } catch (e) {
      self.postMessage({ type: 'error', message: 'Computation error: ' + e.message });
    }
  } else if (msg.type === 'find_all') {
    try {
      const numbers = new Float64Array(msg.numbers);
      const maxResults = msg.maxResults || 1000;

      init_batch_search(numbers, msg.target, msg.minCount, msg.maxCount, maxResults);

      const t0 = performance.now();
      let allCombinations = [];

      function runBatch() {
        try {
          const batchJson = search_batch(NODES_PER_BATCH);
          const batch = JSON.parse(batchJson);

          if (batch.error) {
            self.postMessage({ type: 'error', message: batch.error });
            destroy_batch_search();
            return;
          }

          // Collect new results
          if (batch.new_results && batch.new_results.length > 0) {
            allCombinations = allCombinations.concat(batch.new_results);
          }

          const elapsed = performance.now() - t0;

          // Send progress update
          self.postMessage({
            type: 'progress',
            data: {
              total_found: batch.total_found,
              nodes_explored: batch.nodes_explored,
              progress: batch.progress,
              elapsed_ms: elapsed,
              new_results: batch.new_results || [],
            }
          });

          if (batch.finished) {
            // Send final result
            const finalResult = {
              status: allCombinations.length > 0 ? 'found' : 'not_found',
              combinations: allCombinations,
              total: allCombinations.length,
              elapsed_ms: elapsed,
            };
            self.postMessage({ type: 'result', data: finalResult });
            destroy_batch_search();
          } else {
            // Yield to the event loop so cancel messages can be processed,
            // then continue with the next batch.
            setTimeout(runBatch, 0);
          }
        } catch (err) {
          self.postMessage({ type: 'error', message: 'Batch error: ' + err.message });
          destroy_batch_search();
        }
      }

      // Start the first batch
      runBatch();
    } catch (e) {
      self.postMessage({ type: 'error', message: 'Computation error: ' + e.message });
    }
  }
};
