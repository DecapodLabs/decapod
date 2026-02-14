# MEMORY.md - Memory Architecture

**Authority:** guidance (memory management, optimization, and resource patterns)
**Layer:** Guides
**Binding:** No
**Scope:** memory hierarchy, allocation strategies, and memory optimization
**Non-goals:** language-specific garbage collection details, premature optimization

---

## 1. Memory Hierarchy

### 1.1 The Memory Pyramid

```
Speed:    Fast ←———————————————————————————→ Slow
Size:     Small ←——————————————————————————→ Large
Cost:     High ←———————————————————————————→ Low

Registers    → L1 Cache → L2 Cache → L3 Cache → DRAM → SSD → HDD
1 KB         → 32 KB    → 256 KB   → 8 MB     → 64GB → 1TB → 10TB
1 cycle      → 4 cycles → 10 cycles→ 40 cycles→ 100ns→ 10μs→ 10ms
```

### 1.2 Access Patterns Matter
- **Sequential:** 10x faster than random (cache prefetching)
- **Locality:** Temporal (reuse) and spatial (nearby)
- **Alignment:** Unaligned access = multiple cache lines

---

## 2. Memory Allocation Strategies

### 2.1 Stack Allocation
**When to use:**
- Small, fixed-size objects
- Function-local variables
- RAII patterns
- Deterministic lifetime

**Benefits:**
- Fast allocation (pointer bump)
- Automatic deallocation
- Cache-friendly (sequential)
- No fragmentation

**Limitations:**
- Limited size (platform-dependent)
- Fixed at compile time
- Function scope only

### 2.2 Heap Allocation
**When to use:**
- Dynamic-sized objects
- Long-lived data
- Large objects
- Complex data structures

**Strategies:**
- **Pools:** Pre-allocate, reuse objects (reduces GC/fragmentation)
- **Arenas:** Allocate in bulk, free all at once
- **Slabs:** Fixed-size object caches
- **Buddy systems:** Power-of-2 allocations

### 2.3 Off-Heap Memory
**When to use:**
- Large datasets (GBs)
- Native interop
- Zero-copy I/O
- Shared memory between processes

**Technologies:**
- Memory-mapped files
- Direct ByteBuffers (Java)
- Unsafe/Native memory (various langs)
- Shared memory (shm)

---

## 3. Memory Optimization Patterns

### 3.1 Object Pooling
**Use when:**
- High allocation rate
- Object creation is expensive
- Objects have similar size/lifetime

**Examples:**
- Thread pools
- Connection pools
- Byte buffer pools
- Game object pools

### 3.2 Flyweight Pattern
**Use when:**
- Many similar objects
- Objects can share state
- Memory is constraint

**Examples:**
- Text rendering (glyph sharing)
- Game sprites
- String interning

### 3.3 Lazy Loading
**Use when:**
- Object is expensive to create
- Object may not be needed
- Startup time matters

**Trade-offs:**
- Lower memory footprint
- Higher latency on first access
- Thread safety complexity

### 3.4 Memory-Mapped Files
**Use when:**
- Large file I/O
- Random access to file
- Multiple processes need access
- OS caching desirable

**Benefits:**
- Zero-copy I/O
- OS-managed caching
- Paging handled automatically

---

## 4. Garbage Collection (GC) Considerations

### 4.1 GC-Friendly Patterns
- **Minimize allocations:** Reuse objects, use value types
- **Avoid large objects:** Trigger full GC, fragmentation
- **Short-lived objects:** Cheap in generational GC
- **Object graphs:** Shallow > deep (mark phase)
- **Finalizers:** Avoid, cause resurrection and delays

### 4.2 GC Tuning Strategies
- **Generational:** Separate young/old objects
- **Concurrent:** Minimize pause times
- **Incremental:** Spread work over time
- **Region-based:** G1, ZGC, Shenandoah

### 4.3 Memory Leaks (in GC'd languages)
**Common causes:**
- Static collections growing unbounded
- Event listeners not removed
- Thread-local variables
- Classloader leaks
- Native memory not freed

**Detection:**
- Heap dumps
- Profiling tools
- Memory metrics monitoring
- Leak detection libraries

---

## 5. Memory-Bound Algorithms

### 5.1 External Sorting
When data doesn't fit in memory:
- Chunk data, sort chunks
- K-way merge of sorted chunks

### 5.2 Streaming Processing
- Process data in chunks
- Constant memory regardless of input size
- Examples: Unix pipes, Kafka streams

### 5.3 Approximation Algorithms
When exact answer requires too much memory:
- HyperLogLog for cardinality
- Bloom filters for membership
- Count-Min sketch for frequency
- T-Digest for percentiles

---

## 6. Memory Safety

### 6.1 Buffer Overflows
**Prevention:**
- Bounds checking
- Safe APIs (strncpy vs strcpy)
- Static analysis
- Fuzz testing

### 6.2 Use-After-Free
**Prevention:**
- Smart pointers (RAII)
- Borrow checker (Rust)
- Null pointers after free
- AddressSanitizer

### 6.3 Memory Leaks (all languages)
**Prevention:**
- Clear ownership semantics
- Resource management patterns
- Static analysis
- Continuous profiling

---

## 7. Performance Monitoring

### 7.1 Key Metrics
- **Heap usage:** Current vs max
- **GC frequency:** Collections per minute
- **GC pause times:** P50, P95, P99
- **Allocation rate:** Objects/bytes per second
- **Memory pressure:** Page faults, swap usage

### 7.2 Profiling Tools
- **Heap profilers:** Visualize object graphs
- **Allocation profilers:** Find hot allocation sites
- **Memory leak detectors:** Track unreleased memory
- **Native profilers:** valgrind, perf, Instruments

### 7.3 Optimization Process
1. Measure (don't guess)
2. Identify bottleneck
3. Optimize
4. Verify improvement
5. Repeat

---

## 8. Anti-Patterns

- **Premature optimization:** Measure first
- **Memory hoarding:** Keep everything forever
- **Giant objects:** Violate cache lines
- **Allocation in hot loops:** Create GC pressure
- **Ignoring memory hierarchy:** Random access patterns
- **No bounds checking:** Security vulnerabilities
- **Deep call stacks:** Stack overflow risk
- **Unbounded caches:** Memory leaks

---

## Links

- `embedded/methodology/ARCHITECTURE.md` - CTO-level architecture doctrine
- `embedded/architecture/DATA.md` - Data architecture
- `embedded/architecture/CACHING.md` - Caching patterns
- `embedded/architecture/CONCURRENCY.md` - Shared memory patterns
