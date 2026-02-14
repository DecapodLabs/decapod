# ALGORITHMS.md - Algorithms & Data Structures

**Authority:** guidance (algorithm selection, complexity analysis, and optimization)
**Layer:** Guides
**Binding:** No
**Scope:** algorithm patterns, complexity trade-offs, and data structure selection
**Non-goals:** academic proofs, premature optimization without measurement

---

## 1. Algorithm Selection Principles

### 1.1 Measure First, Optimize Second
**Premature optimization is the root of all evil.**
- Profile before optimizing
- Optimize bottlenecks, not everything
- Constant factors matter in practice
- Cache efficiency > Big-O for small n

### 1.2 The Right Data Structure
**Programs = Algorithms + Data Structures**
- Algorithm choice depends on data structure
- Data structure choice depends on access patterns
- Space-time trade-offs
- Cache-friendly vs cache-oblivious

### 1.3 Practical vs Theoretical
- **Big-O:** Asymptotic behavior
- **Cache:** Memory hierarchy matters
- **Parallelism:** Amdahl's Law limits
- **Constants:** 2× slower is still O(n)

---

## 2. Complexity Analysis

### 2.1 Time Complexity

| Complexity | Name | Practical Limit | Examples |
|------------|------|-----------------|----------|
| O(1) | Constant | Unlimited | Hash map access |
| O(log n) | Logarithmic | Millions | Binary search |
| O(n) | Linear | Billions | Single loop |
| O(n log n) | Linearithmic | Millions | Sorting |
| O(n²) | Quadratic | Thousands | Nested loops |
| O(2ⁿ) | Exponential | < 30 | Brute force |
| O(n!) | Factorial | < 12 | Permutations |

### 2.2 Space Complexity
- **In-place:** O(1) extra space
- **Linear:** O(n) space
- **Recursion:** Call stack depth
- **Cache:** Working set size

### 2.3 Amortized Analysis
- **Average case:** Over sequence of operations
- **Example:** Dynamic array doubling (amortized O(1) append)
- **Worst case:** Single operation cost

---

## 3. Fundamental Algorithms

### 3.1 Searching
**Linear Search:**
- O(n) time, O(1) space
- Unsorted data, small datasets

**Binary Search:**
- O(log n) time, O(1) space
- Sorted data, random access
- Variants: lower_bound, upper_bound

**Hash-based Lookup:**
- O(1) average, O(n) worst
- Unsorted data, unique keys
- Trade-off: space for time

### 3.2 Sorting
**Comparison Sorts:**
- **Quicksort:** O(n log n) avg, O(n²) worst, in-place
- **Mergesort:** O(n log n), stable, not in-place
- **Heapsort:** O(n log n), in-place, not stable
- **Timsort:** O(n log n), adaptive, stable (Python, Java)

**Non-Comparison Sorts:**
- **Counting sort:** O(n + k), integer keys
- **Radix sort:** O(nk), integer keys
- **Bucket sort:** O(n), uniform distribution

**When to use what:**
- Default: Language's built-in sort (optimized)
- Large datasets: External sort
- Nearly sorted: Insertion sort, Timsort
- Linked lists: Mergesort

### 3.3 Graph Algorithms
**Graph Representations:**
- **Adjacency matrix:** O(V²) space, fast edge lookup
- **Adjacency list:** O(V + E) space, sparse graphs

**Traversal:**
- **BFS:** Shortest path (unweighted), level-order
- **DFS:** Topological sort, cycle detection, connected components

**Shortest Path:**
- **Dijkstra:** Single source, non-negative weights, O((V + E) log V)
- **Bellman-Ford:** Single source, negative weights, O(VE)
- **Floyd-Warshall:** All pairs, O(V³)
- **A*:** Heuristic-guided, pathfinding

**Minimum Spanning Tree:**
- **Kruskal:** O(E log E), edge list
- **Prim:** O(E log V), adjacency list

---

## 4. Data Structures

### 4.1 Arrays and Lists
**Arrays:**
- O(1) random access
- O(n) insert/delete
- Cache-friendly

**Linked Lists:**
- O(n) random access
- O(1) insert/delete (known position)
- Poor cache locality

**Dynamic Arrays (Vector/ArrayList):**
- Amortized O(1) append
- O(n) worst case (resize)
- Most practical choice

### 4.2 Stacks and Queues
**Stack (LIFO):**
- Push, pop: O(1)
- Use: DFS, expression evaluation, undo

**Queue (FIFO):**
- Enqueue, dequeue: O(1)
- Use: BFS, task scheduling, buffering

**Deque:**
- Double-ended operations
- O(1) at both ends

**Priority Queue:**
- Insert: O(log n)
- Extract-min/max: O(log n)
- Heap implementation

### 4.3 Trees
**Binary Search Tree (BST):**
- O(log n) avg, O(n) worst (unbalanced)
- In-order traversal = sorted

**Balanced BSTs:**
- **AVL:** Strictly balanced, faster lookups
- **Red-Black:** Loosely balanced, faster inserts
- **B-Trees:** Optimized for disk, databases

**Heaps:**
- Complete binary tree
- Min-heap or max-heap
- Priority queue implementation
- Heapify: O(n)

**Tries (Prefix Trees):**
- String storage
- O(m) lookup (m = string length)
- Autocomplete, spell check

### 4.4 Hash Tables
- O(1) average lookup
- O(n) worst case (collisions)
- Load factor determines performance
- Collision resolution: chaining vs open addressing

### 4.5 Graph Representations
- **Adjacency matrix:** Dense graphs
- **Adjacency list:** Sparse graphs
- **Edge list:** Kruskal's algorithm

---

## 5. Advanced Algorithms

### 5.1 Dynamic Programming
**When to use:**
- Optimal substructure
- Overlapping subproblems
- Can be memoized or tabulated

**Examples:**
- Fibonacci
- Knapsack
- Longest Common Subsequence
- Edit Distance
- Matrix Chain Multiplication

**Approaches:**
- **Top-down:** Recursion + memoization
- **Bottom-up:** Iterative tabulation

### 5.2 Greedy Algorithms
**When to use:**
- Greedy choice property
- Optimal substructure
- Local optimum = global optimum

**Examples:**
- Dijkstra's algorithm
- Huffman coding
- Activity selection
- Fractional knapsack

### 5.3 Divide and Conquer
**Pattern:**
1. Divide problem into subproblems
2. Conquer subproblems recursively
3. Combine solutions

**Examples:**
- Mergesort
- Quicksort
- Binary search
- Strassen's matrix multiplication
- Fast Fourier Transform (FFT)

### 5.4 Backtracking
**When to use:**
- Search all possible solutions
- Constraint satisfaction
- Can prune invalid branches

**Examples:**
- N-Queens
- Sudoku solver
- Subset sum
- Graph coloring

---

## 6. Probabilistic Data Structures

### 6.1 Bloom Filter
- **Space:** O(n), n = expected elements
- **Time:** O(k), k = hash functions
- **Use:** Membership testing, cache filtering
- **Trade-off:** False positives possible, no false negatives

### 6.2 HyperLogLog
- **Space:** O(1), ~1.5KB
- **Time:** O(1) per element
- **Use:** Cardinality estimation
- **Accuracy:** ~2% error

### 6.3 Count-Min Sketch
- **Space:** O(w × d), w = width, d = depth
- **Time:** O(d) per operation
- **Use:** Frequency estimation
- **Trade-off:** Overestimates possible

### 6.4 Skip List
- **Time:** O(log n) average
- **Space:** O(n)
- **Use:** Ordered set/map, simpler than BST
- **Benefits:** Lock-free implementations possible

### 6.5 T-Digest
- **Space:** O(1), configurable accuracy
- **Time:** O(1) per observation
- **Use:** Percentile estimation
- **Accuracy:** High accuracy at tails

---

## 7. Algorithm Patterns

### 7.1 Two Pointers
- **Use:** Sorted arrays, palindromes, sliding window
- **Time:** O(n)
- **Space:** O(1)

### 7.2 Sliding Window
- **Use:** Subarray problems, string processing
- **Time:** O(n)
- **Variants:** Fixed size, variable size

### 7.3 Fast and Slow Pointers
- **Use:** Cycle detection (Floyd's algorithm)
- **Time:** O(n)
- **Space:** O(1)

### 7.4 Merge Intervals
- **Use:** Overlapping intervals, scheduling
- **Time:** O(n log n)
- **Pattern:** Sort, then merge

### 7.5 Cyclic Sort
- **Use:** Arrays with values in range [1, n]
- **Time:** O(n)
- **Space:** O(1)

### 7.6 Topological Sort
- **Use:** Dependency ordering, task scheduling
- **Time:** O(V + E)
- **Algorithm:** Kahn's or DFS-based

---

## 8. Optimization Strategies

### 8.1 Space Optimization
- **In-place:** Modify input instead of copy
- **Bit manipulation:** Compact representation
- **Streaming:** Process data in chunks

### 8.2 Time Optimization
- **Memoization:** Cache results
- **Precomputation:** Compute once, use many
- **Early exit:** Fail fast
- **Pruning:** Skip unnecessary work

### 8.3 Parallel Optimization
- **Map-Reduce:** Distributed processing
- **SIMD:** Vectorized operations
- **GPU:** Massive parallelism

---

## 9. Anti-Patterns

- **Premature optimization:** Optimize without profiling
- **Wrong data structure:** Array for frequent inserts
- **O(n²) when O(n log n) possible:** Nested loops on sorted data
- **Brute force:** When DP or greedy applies
- **Ignoring cache:** Linked lists for sequential access
- **Recursion without base case:** Stack overflow
- **Unbounded recursion:** Convert to iteration
- **No early termination:** Continue when answer found
- **Recomputing values:** No memoization
- **Over-engineering:** Complex algorithm for simple problem

---

## Links

- `embedded/methodology/ARCHITECTURE.md` - CTO-level architecture doctrine
- `embedded/architecture/MEMORY.md` - Memory and cache efficiency
- `embedded/architecture/CONCURRENCY.md` - Parallel algorithms
- `embedded/architecture/PERFORMANCE.md` - Performance optimization
