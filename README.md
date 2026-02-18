# Target Sum Finder

A high-performance web app that finds combinations of numbers summing to a target value, powered by **WebAssembly** (Rust) running in a **Web Worker**.

Upload a CSV of numbers, set a target, and get results in milliseconds. Supports numbers up to 10^12, inputs up to 1M rows, bilingual UI (English/Farsi), live progress streaming, and cancellable searches.

## Quick Start

```bash
# Any static file server works
python3 -m http.server 8080
# Open http://localhost:8080
```

No build step needed -- the pre-compiled WASM module is included in `wasm-solver/pkg/`.

## Deploy to Railway

```bash
# Push to GitHub, then connect the repo to Railway.
# Or use the Railway CLI:
railway up
```

Railway config is in `railway.toml`. The `Dockerfile` uses nginx to serve the static files with correct WASM MIME types.

### Rebuilding the WASM Module

```bash
cd wasm-solver
cargo install wasm-pack
wasm-pack build --target web --release
```

## Algorithm

### Hybrid Strategy

| Input Size (n) | Algorithm | Why |
|---|---|---|
| n <= 40 | **Meet-in-the-middle** | Optimal for small n with any target size. Splits input in half, enumerates 2^(n/2) subsets per half, finds pairs via hash lookup. |
| n > 40 | **Branch-and-bound DFS** | Handles large n with aggressive pruning to cut exponential blowup. |

### Why Not Standard DP?

Standard subset-sum DP needs `O(target)` memory. With targets up to 10^12, that's terabytes -- impossible.

### Pruning (Branch-and-Bound)

1. **Value bound** -- element exceeds remaining budget (sorted, so entire subtree pruned)
2. **Suffix sum** -- all remaining elements can't reach target
3. **Count bounds** -- exceeds max_count or can't reach min_count
4. **Early exit** -- stop on first valid combination (default mode)

### Streaming "Find All"

The "find all" mode uses a **resumable batch search**. The DFS is converted to an explicit stack so WASM can yield control back to JS every ~200K nodes. This enables:

- Live progress bar and node count
- Results streamed into the UI as they're found
- Cancellation between batches (worker termination)

## Complexity

| Algorithm | Time | Space |
|---|---|---|
| Meet-in-the-middle | O(2^(n/2)) | O(2^(n/2)) |
| Branch-and-bound | O(2^n) worst, much better with pruning | O(n) stack |

## Limits

- **Numbers**: positive integers up to 2^53 (10^12 is well within range)
- **CSV**: up to 1M rows
- **Target**: up to 2^53
- **WASM memory**: bounded, no unbounded allocations
- **Performance**: depends on input distribution; pathological cases can be slow

## Architecture

```
index.html              -- UI (bilingual English/Farsi, dark theme)
worker.js               -- Web Worker (WASM loader, batch loop, progress reporting)
Dockerfile              -- nginx-based container for Railway / any Docker host
railway.toml            -- Railway deployment config
wasm-solver/
  src/
    lib.rs              -- WASM bindings (JS <-> Rust interface)
    solver.rs           -- Core algorithms (MITM + B&B for find-one)
    batch.rs            -- Resumable batch DFS (for streaming find-all)
    utils.rs            -- Panic hook
  pkg/                  -- Compiled WASM output (43KB)
```

### WASM <-> JS Interface

- **JS -> WASM**: `Float64Array` of numbers, scalar params
- **WASM -> JS**: JSON strings (parsed in JS)
- **Batch API**: `init_batch_search()` -> loop `search_batch(budget)` -> `destroy_batch_search()`
- **Cancel**: main thread terminates + recreates the worker
- **Threading**: WASM runs in a Web Worker, UI thread is never blocked

## Extending

- **Bitset DP**: for cases where target is small enough after GCD reduction
- **Parallel MITM**: split work across multiple workers for n <= 50
- **Randomized search**: approximate solutions for infeasible exact searches

## UI

- Bilingual (English + Farsi/RTL)
- Drag-and-drop CSV upload
- Live progress bar + streamed results for "find all"
- Cancellable at any point
- Clear validation errors
- Results show original CSV row numbers, values, and verified sum
