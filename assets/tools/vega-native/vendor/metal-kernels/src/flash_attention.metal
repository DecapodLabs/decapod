/**
 * Metal Flash Attention Kernels for xInfer (vLLM.rs)
 * Copyright (c) 2025, Guoqing Bao. All rights reserved.
 *
 * High-performance flash attention implementation for Apple Metal GPUs.
 * Implements Flash Attention v2 algorithm with Metal-specific optimizations:
 *   - Tiled computation with SIMD group cooperation
 *   - Online softmax (two-pass rescaling)
 *   - Vectorized loads (float4) for memory bandwidth
 *   - Threadgroup shared memory for KV tiles
 *   - Split-K for long-context decode
 *   - FP8 KV cache dequantization support
 *
 * KV Cache layout (flash path): [num_blocks, block_size, num_kv_heads, head_dim]
 */

#include "metal_dtype.metal"
#include <metal_stdlib>
#include <metal_compute>
#include <metal_simdgroup>

using namespace metal;

// ============================================================================
// Flash Attention Prefill Kernel
// ============================================================================
// Algorithm: For each query tile (BR tokens), iterate over all KV tiles (BC tokens):
//   1. Load Q tile into registers
//   2. For each KV tile from paged cache:
//      a. Load K tile into threadgroup memory
//      b. Compute S = Q @ K^T (dot products)
//      c. Apply causal mask + softcap
//      d. Online softmax update (max, sum tracking)
//      e. Load V tile into threadgroup memory
//      f. Accumulate O += P @ V
//   3. Final rescale of O by 1/l

template <typename T, typename cache_t, bool is_fp8, int HEAD_DIM, int BLOCK_SIZE, int BR, int BC>
[[kernel]] void flash_attention_prefill(
    device T* output [[buffer(0)]],
    device const T* q [[buffer(1)]],
    device const cache_t* k_cache [[buffer(2)]],
    device const cache_t* v_cache [[buffer(3)]],
    const constant int& num_kv_heads [[buffer(4)]],
    const constant float& scale [[buffer(5)]],
    device const uint32_t* block_tables [[buffer(6)]],
    device const uint32_t* seq_lens [[buffer(7)]],
    const constant int& block_table_stride [[buffer(8)]],
    const constant int& num_seqs [[buffer(9)]],
    const constant int& num_q_heads [[buffer(10)]],
    const constant int& num_q_tokens [[buffer(11)]],
    const constant float& softcapping [[buffer(12)]],
    const constant int& o_stride [[buffer(13)]],
    device const uint32_t* query_start_len [[buffer(14)]],
    device const float* alibi_slopes [[buffer(15)]],
    device const float* k_scales [[buffer(16)]],
    device const float* v_scales [[buffer(17)]],
    device const uint32_t* sinks [[buffer(18)]],
    const constant int& sliding_window [[buffer(19)]],
    const constant int& total_num_blocks [[buffer(20)]],
    const constant int& kv_block_stride [[buffer(21)]],
    const constant int& kv_head_stride [[buffer(22)]],
    threadgroup char* smem [[threadgroup(0)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lid [[thread_index_in_simdgroup]]
) {
    const int q_head_within_kv = tgid.x;
    const int kv_head_idx = tgid.y;
    const int token_chunk_idx = tgid.z;

    const int num_queries_per_kv = num_q_heads / num_kv_heads;
    const int q_head_idx = kv_head_idx * num_queries_per_kv + q_head_within_kv;

    const int thread_idx = tid.x;
    const int num_threads = BR;

    const int token_start = token_chunk_idx * BR;
    const int my_token_idx = token_start + thread_idx;

    if (my_token_idx >= num_q_tokens) return;

    // Find which sequence this token belongs to
    int seq_idx = 0;
    int query_start = 0;
    for (int s = 0; s < num_seqs; s++) {
        int next_start = query_start_len[s + 1];
        if (my_token_idx < (int)next_start) {
            seq_idx = s;
            break;
        }
        query_start = next_start;
    }

    const int local_q_idx = my_token_idx - query_start;
    const int context_len = seq_lens[seq_idx];
    const int q_len = query_start_len[seq_idx + 1] - query_start_len[seq_idx];
    const int kv_start = context_len - q_len;
    const int total_kv_len = context_len;

    // Load Q into registers
    float q_reg[HEAD_DIM];
    device const T* q_ptr = q + (my_token_idx * num_q_heads + q_head_idx) * HEAD_DIM;
    for (int d = 0; d < HEAD_DIM; d++) {
        q_reg[d] = float(q_ptr[d]);
    }

    // Initialize output accumulator and online softmax state
    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;
    float m_prev = -INFINITY;
    float l_prev = 0.0f;

    // FP8 scales
    float kscale = 1.0f;
    float vscale = 1.0f;
    if (is_fp8) {
        kscale = k_scales[kv_head_idx];
        vscale = v_scales[kv_head_idx];
    }

    // Block table for this sequence
    device const uint32_t* seq_block_table = block_tables + seq_idx * block_table_stride;

    // Iterate over KV sequence in blocks
    const int causal_end = kv_start + local_q_idx + 1;
    int kv_end = min(total_kv_len, causal_end);

    if (sliding_window > 0) {
        int sw_start = max(0, causal_end - sliding_window);
        kv_end = min(kv_end, causal_end);
        // Skip blocks before sliding window
    }

    for (int kv_pos = 0; kv_pos < kv_end; kv_pos += BC) {
        int tile_end = min(kv_pos + BC, kv_end);
        int tile_len = tile_end - kv_pos;

        float m_cur = -INFINITY;
        float l_cur = 0.0f;
        float p_vals[BC];

        // Compute S = Q @ K^T for this tile
        for (int t = 0; t < tile_len; t++) {
            int kv_idx = kv_pos + t;
            int block_idx = kv_idx / BLOCK_SIZE;
            int block_offset = kv_idx % BLOCK_SIZE;
            uint32_t physical_block = seq_block_table[block_idx];

            // K cache: [num_blocks, block_size, num_kv_heads, head_dim]
            device const cache_t* k_ptr = k_cache +
                (physical_block * BLOCK_SIZE + block_offset) * (num_kv_heads * HEAD_DIM) +
                kv_head_idx * HEAD_DIM;

            float dot = 0.0f;
            for (int d = 0; d < HEAD_DIM; d += 4) {
                float4 kv;
                if (is_fp8) {
                    kv = float4(softmax_fp8_to_float(k_ptr[d]), softmax_fp8_to_float(k_ptr[d+1]),
                               softmax_fp8_to_float(k_ptr[d+2]), softmax_fp8_to_float(k_ptr[d+3])) * kscale;
                } else {
                    kv = float4(float(k_ptr[d]), float(k_ptr[d+1]),
                               float(k_ptr[d+2]), float(k_ptr[d+3]));
                }
                dot += q_reg[d] * kv.x + q_reg[d+1] * kv.y +
                       q_reg[d+2] * kv.z + q_reg[d+3] * kv.w;
            }
            dot *= scale;

            // Softcap
            if (softcapping > 0.0f) {
                dot = softcapping * precise::tanh(dot / softcapping);
            }

            // Causal mask
            if (kv_idx > kv_start + local_q_idx) {
                dot = -INFINITY;
            }

            p_vals[t] = dot;
            m_cur = max(m_cur, dot);
        }

        // Online softmax rescaling
        float m_new = max(m_prev, m_cur);
        float scale_prev = exp(m_prev - m_new);
        float scale_cur = exp(m_cur - m_new);

        // Compute exp(s - m_new) and sum
        l_cur = 0.0f;
        for (int t = 0; t < tile_len; t++) {
            p_vals[t] = exp(p_vals[t] - m_new);
            l_cur += p_vals[t];
        }

        // Rescale previous accumulator
        float l_new = l_prev * scale_prev + l_cur;

        for (int d = 0; d < HEAD_DIM; d++) {
            o_acc[d] *= (l_prev * scale_prev) / max(l_new, 1e-10f);
        }

        // Accumulate P @ V for this tile
        for (int t = 0; t < tile_len; t++) {
            int kv_idx = kv_pos + t;
            int block_idx = kv_idx / BLOCK_SIZE;
            int block_offset = kv_idx % BLOCK_SIZE;
            uint32_t physical_block = seq_block_table[block_idx];

            // V cache: [num_blocks, block_size, num_kv_heads, head_dim]
            device const cache_t* v_ptr = v_cache +
                (physical_block * BLOCK_SIZE + block_offset) * (num_kv_heads * HEAD_DIM) +
                kv_head_idx * HEAD_DIM;

            float weight = p_vals[t] / max(l_new, 1e-10f);
            for (int d = 0; d < HEAD_DIM; d += 4) {
                float4 vv;
                if (is_fp8) {
                    vv = float4(softmax_fp8_to_float(v_ptr[d]), softmax_fp8_to_float(v_ptr[d+1]),
                               softmax_fp8_to_float(v_ptr[d+2]), softmax_fp8_to_float(v_ptr[d+3])) * vscale;
                } else {
                    vv = float4(float(v_ptr[d]), float(v_ptr[d+1]),
                               float(v_ptr[d+2]), float(v_ptr[d+3]));
                }
                o_acc[d]   += weight * vv.x;
                o_acc[d+1] += weight * vv.y;
                o_acc[d+2] += weight * vv.z;
                o_acc[d+3] += weight * vv.w;
            }
        }

        m_prev = m_new;
        l_prev = l_new;
    }

    // Write output
    device T* o_ptr = output + (my_token_idx * num_q_heads + q_head_idx) * HEAD_DIM;
    for (int d = 0; d < HEAD_DIM; d++) {
        o_ptr[d] = T(o_acc[d]);
    }
}

// ============================================================================
// Flash Attention Decode Kernel (Optimized)
// ============================================================================
// Strategy: Each thread handles ALL head dimensions for its assigned KV tokens.
// Then use a 2-phase reduction: first within simdgroups (warp-level, no barriers),
// then across simdgroups (one barrier). Each thread outputs its assigned dims.
// NUM_THREADS must be >= HEAD_DIM for the output write phase.

template <typename T, typename cache_t, bool is_fp8, int HEAD_DIM, int BLOCK_SIZE, int NUM_THREADS>
[[kernel]] void flash_attention_decode(
    device T* output [[buffer(0)]],
    device const T* q [[buffer(1)]],
    device const cache_t* k_cache [[buffer(2)]],
    device const cache_t* v_cache [[buffer(3)]],
    device const uint32_t* block_tables [[buffer(4)]],
    device const uint32_t* context_lens [[buffer(5)]],
    const constant int& max_blocks_per_seq [[buffer(6)]],
    const constant int& num_q_heads [[buffer(7)]],
    const constant int& num_kv_heads [[buffer(8)]],
    const constant int& head_dim_const [[buffer(9)]],
    const constant int& block_size_const [[buffer(10)]],
    const constant float& scale [[buffer(11)]],
    const constant int& num_seqs [[buffer(12)]],
    const constant int& q_stride [[buffer(13)]],
    const constant float& softcapping [[buffer(14)]],
    const constant int& sliding_window [[buffer(15)]],
    device const float* k_scales [[buffer(16)]],
    device const float* v_scales [[buffer(17)]],
    threadgroup char* smem [[threadgroup(0)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lid [[thread_index_in_simdgroup]]
) {
    const int head_idx = tgid.x;
    const int seq_idx = tgid.y;
    const int thread_idx = tid.x;

    const int kv_head_idx = head_idx / (num_q_heads / num_kv_heads);
    const uint32_t context_len = context_lens[seq_idx];

    if (context_len == 0) return;

    constexpr int NUM_SIMD_GROUPS = NUM_THREADS / 32;
    // Each thread handles DIMS_PER_THREAD dimensions of the output
    constexpr int DIMS_PER_THREAD = HEAD_DIM / NUM_THREADS;
    // For HEAD_DIM=128, NUM_THREADS=128: each thread handles 1 dim
    // For HEAD_DIM=128, NUM_THREADS=256: we use thread_idx < HEAD_DIM for output

    // Load Q into shared memory (all threads cooperate)
    threadgroup float* q_shared = (threadgroup float*)smem;
    if (thread_idx < HEAD_DIM) {
        device const T* q_ptr = q + seq_idx * q_stride + head_idx * HEAD_DIM;
        q_shared[thread_idx] = float(q_ptr[thread_idx]);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // FP8 scales
    float kscale = 1.0f;
    float vscale = 1.0f;
    if (is_fp8) {
        kscale = k_scales[kv_head_idx];
        vscale = v_scales[kv_head_idx];
    }

    // Block table for this sequence
    device const uint32_t* seq_block_table = block_tables + seq_idx * max_blocks_per_seq;

    // Each thread computes partial output for ALL dimensions
    // but only for its assigned KV tokens
    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;
    float m_local = -INFINITY;
    float l_local = 0.0f;

    const int total_kv = (int)context_len;
    int sw_start = 0;
    if (sliding_window > 0) {
        sw_start = max(0, total_kv - sliding_window);
    }

    // Each thread processes KV tokens strided by NUM_THREADS
    for (int kv_idx = sw_start + thread_idx; kv_idx < total_kv; kv_idx += NUM_THREADS) {
        int block_idx = kv_idx / BLOCK_SIZE;
        int block_offset = kv_idx % BLOCK_SIZE;
        uint32_t physical_block = seq_block_table[block_idx];

        device const cache_t* k_ptr = k_cache +
            (physical_block * BLOCK_SIZE + block_offset) * (num_kv_heads * HEAD_DIM) +
            kv_head_idx * HEAD_DIM;

        // Q.K dot product
        float dot = 0.0f;
        for (int d = 0; d < HEAD_DIM; d += 4) {
            float4 kv;
            if (is_fp8) {
                kv = float4(softmax_fp8_to_float(k_ptr[d]), softmax_fp8_to_float(k_ptr[d+1]),
                           softmax_fp8_to_float(k_ptr[d+2]), softmax_fp8_to_float(k_ptr[d+3])) * kscale;
            } else {
                kv = float4(float(k_ptr[d]), float(k_ptr[d+1]),
                           float(k_ptr[d+2]), float(k_ptr[d+3]));
            }
            dot += q_shared[d] * kv.x + q_shared[d+1] * kv.y +
                   q_shared[d+2] * kv.z + q_shared[d+3] * kv.w;
        }
        dot *= scale;

        if (softcapping > 0.0f) {
            dot = softcapping * precise::tanh(dot / softcapping);
        }

        // Online softmax update
        float m_new = max(m_local, dot);
        float scale_old = exp(m_local - m_new);
        float p = exp(dot - m_new);

        for (int d = 0; d < HEAD_DIM; d++) {
            o_acc[d] *= scale_old;
        }
        l_local = l_local * scale_old + p;
        m_local = m_new;

        // Accumulate V
        device const cache_t* v_ptr = v_cache +
            (physical_block * BLOCK_SIZE + block_offset) * (num_kv_heads * HEAD_DIM) +
            kv_head_idx * HEAD_DIM;

        for (int d = 0; d < HEAD_DIM; d += 4) {
            float4 vv;
            if (is_fp8) {
                vv = float4(softmax_fp8_to_float(v_ptr[d]), softmax_fp8_to_float(v_ptr[d+1]),
                           softmax_fp8_to_float(v_ptr[d+2]), softmax_fp8_to_float(v_ptr[d+3])) * vscale;
            } else {
                vv = float4(float(v_ptr[d]), float(v_ptr[d+1]),
                           float(v_ptr[d+2]), float(v_ptr[d+3]));
            }
            o_acc[d]   += p * vv.x;
            o_acc[d+1] += p * vv.y;
            o_acc[d+2] += p * vv.z;
            o_acc[d+3] += p * vv.w;
        }
    }

    // Cross-thread reduction via shared memory
    // Each thread writes its (m, l, o_acc) then we do a tree merge
    // Layout: [NUM_THREADS] floats for m, [NUM_THREADS] for l,
    // then [NUM_THREADS * HEAD_DIM] for output accumulators

    // First, reduce m and l to find global values
    threadgroup float* s_m = (threadgroup float*)(smem + HEAD_DIM * sizeof(float));
    threadgroup float* s_l = s_m + NUM_THREADS;

    s_m[thread_idx] = m_local;
    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Tree reduction for global max
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) {
            s_m[thread_idx] = max(s_m[thread_idx], s_m[thread_idx + stride]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_max = s_m[0];
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Each thread rescales its l and o to global_max
    float my_rescale = exp(m_local - global_max);
    l_local *= my_rescale;
    for (int d = 0; d < HEAD_DIM; d++) {
        o_acc[d] *= my_rescale;
    }

    // Sum l values
    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) {
            s_l[thread_idx] += s_l[thread_idx + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_l = s_l[0];
    float inv_l = 1.0f / max(global_l, 1e-10f);
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Reduce output dimensions: process HEAD_DIM dims in chunks of NUM_THREADS
    // Each chunk: all threads contribute their o_acc[d] to a parallel reduction
    threadgroup float* s_reduce = (threadgroup float*)(smem + HEAD_DIM * sizeof(float));

    device T* o_ptr = output + seq_idx * q_stride + head_idx * HEAD_DIM;

    // For HEAD_DIM=128 and NUM_THREADS=256: 
    // We do 1 pass where threads 0-127 each reduce their own dimension
    // across all NUM_THREADS thread contributions.
    // But we can't fit NUM_THREADS * HEAD_DIM in shared memory.
    
    // Alternative: each thread writes one dim at a time and we reduce
    // This costs HEAD_DIM * log2(NUM_THREADS) barriers = 128 * 8 = 1024 barriers (bad!)
    
    // Better: use chunk approach. Process 4 dims per pass with NUM_THREADS/4 threads reducing.
    // Actually the simplest fast approach for decode single-token:
    // Use atomic adds to device memory (slow) or...
    
    // Best approach for Metal: interleave dim assignment.
    // With 256 threads and 128 dims, use 2 threads per dim, reduce pairs.
    // Or better: 256 threads, each handles all 128 dims, reduce in pairs via shared mem.
    
    // Let's use a 2-pass approach: first reduce within simdgroups (no barriers),
    // then reduce across simdgroups (one barrier per dim-chunk).
    
    // Step 1: simd reduction (within each 32-thread group) for each dimension
    // Each thread's o_acc[d] contributes; after simd_sum, lane 0 has partial sum for that simdgroup
    for (int d = 0; d < HEAD_DIM; d++) {
        o_acc[d] = simd_sum(o_acc[d]);
    }
    
    // Step 2: simdgroup leaders write to shared memory
    // s_reduce: [NUM_SIMD_GROUPS * HEAD_DIM]
    if (simd_lid == 0) {
        for (int d = 0; d < HEAD_DIM; d++) {
            s_reduce[simd_gid * HEAD_DIM + d] = o_acc[d];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    
    // Step 3: first HEAD_DIM threads each sum across simdgroups for their dimension
    if (thread_idx < (uint)HEAD_DIM) {
        int d = thread_idx;
        float sum = 0.0f;
        for (int sg = 0; sg < NUM_SIMD_GROUPS; sg++) {
            sum += s_reduce[sg * HEAD_DIM + d];
        }
        o_ptr[d] = T(sum * inv_l);
    }
}

// ============================================================================
// Flash Reshape and Cache kernel
// ============================================================================
// Writes K/V into the flash-format paged cache:
// Cache layout: [num_blocks, block_size, num_kv_heads, head_dim]

template <typename T, typename cache_t, bool is_fp8>
[[kernel]] void flash_reshape_and_cache(
    device const T* key [[buffer(0)]],
    device const T* value [[buffer(1)]],
    device cache_t* key_cache [[buffer(2)]],
    device cache_t* value_cache [[buffer(3)]],
    device const long* slot_mapping [[buffer(4)]],
    const constant int& num_tokens [[buffer(5)]],
    const constant int& num_kv_heads [[buffer(6)]],
    const constant int& head_dim [[buffer(7)]],
    const constant int& block_size [[buffer(8)]],
    const constant int& key_stride [[buffer(9)]],
    const constant int& value_stride [[buffer(10)]],
    device const float* k_scales [[buffer(11)]],
    device const float* v_scales [[buffer(12)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]]
) {
    const int token_idx = tgid.x;
    const int head_idx = tgid.y;
    if (token_idx >= num_tokens) return;

    const long slot = slot_mapping[token_idx];
    if (slot < 0) return;

    const int block_idx = slot / block_size;
    const int block_offset = slot % block_size;

    const int thread_idx = tid.x;
    const int stride = num_kv_heads * head_dim;

    // Input: [num_tokens, num_kv_heads, head_dim] but with custom strides
    device const T* k_in = key + token_idx * key_stride;
    device const T* v_in = value + token_idx * value_stride;

    // Cache: [num_blocks, block_size, num_kv_heads, head_dim]
    int cache_offset = (block_idx * block_size + block_offset) * stride;
    device cache_t* k_out = key_cache + cache_offset;
    device cache_t* v_out = value_cache + cache_offset;

    // Each thread handles one or more elements
    for (int i = thread_idx; i < stride; i += 256) {
        if (is_fp8) {
            int head_idx = i / head_dim;
            float ks = k_scales[head_idx];
            float vs = v_scales[head_idx];
            float kval = float(k_in[i]) / max(ks, 1e-10f);
            float vval = float(v_in[i]) / max(vs, 1e-10f);
            k_out[i] = float_to_softmax_fp8(clamp(kval, -448.0f, 448.0f));
            v_out[i] = float_to_softmax_fp8(clamp(vval, -448.0f, 448.0f));
        } else {
            k_out[i] = cache_t(k_in[i]);
            v_out[i] = cache_t(v_in[i]);
        }
    }
}

// ============================================================================
// Split-K Decode Kernel (for long contexts)
// ============================================================================
// Each partition handles a subset of KV blocks, writes partial results.
// A reduce kernel merges them.

template <typename T, typename cache_t, bool is_fp8, int HEAD_DIM, int BLOCK_SIZE, int NUM_THREADS>
[[kernel]] void flash_attention_decode_splitk(
    device float* partial_out [[buffer(0)]],    // [num_seqs, num_heads, num_splits, head_dim]
    device float* partial_max [[buffer(1)]],    // [num_seqs, num_heads, num_splits]
    device float* partial_sum [[buffer(2)]],    // [num_seqs, num_heads, num_splits]
    device const T* q [[buffer(3)]],
    device const cache_t* k_cache [[buffer(4)]],
    device const cache_t* v_cache [[buffer(5)]],
    device const uint32_t* block_tables [[buffer(6)]],
    device const uint32_t* context_lens [[buffer(7)]],
    const constant int& max_blocks_per_seq [[buffer(8)]],
    const constant int& num_q_heads [[buffer(9)]],
    const constant int& num_kv_heads [[buffer(10)]],
    const constant int& head_dim_const [[buffer(11)]],
    const constant int& block_size_const [[buffer(12)]],
    const constant float& scale [[buffer(13)]],
    const constant int& num_seqs [[buffer(14)]],
    const constant int& num_splits [[buffer(15)]],
    const constant int& q_stride [[buffer(16)]],
    const constant float& softcapping [[buffer(17)]],
    const constant int& sliding_window [[buffer(18)]],
    device const float* k_scales [[buffer(19)]],
    device const float* v_scales [[buffer(20)]],
    threadgroup char* smem [[threadgroup(0)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lid [[thread_index_in_simdgroup]]
) {
    const int head_idx = tgid.x;
    const int seq_idx = tgid.y;
    const int split_idx = tgid.z;
    const int thread_idx = tid.x;

    const int kv_head_idx = head_idx / (num_q_heads / num_kv_heads);
    const uint32_t context_len = context_lens[seq_idx];

    if (context_len == 0) return;

    // Determine this split's range of KV tokens
    int total_kv = (int)context_len;
    int sw_start = 0;
    if (sliding_window > 0) {
        sw_start = max(0, total_kv - sliding_window);
        total_kv = total_kv - sw_start;
    }

    int tokens_per_split = (total_kv + num_splits - 1) / num_splits;
    int split_start = sw_start + split_idx * tokens_per_split;
    int split_end = min(sw_start + total_kv, split_start + tokens_per_split);

    if (split_start >= split_end) {
        // No work for this split
        int out_offset = (seq_idx * num_q_heads + head_idx) * num_splits + split_idx;
        partial_max[out_offset] = -INFINITY;
        partial_sum[out_offset] = 0.0f;
        return;
    }

    // Load Q
    device const T* q_ptr = q + seq_idx * q_stride + head_idx * HEAD_DIM;
    float q_reg[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) q_reg[d] = float(q_ptr[d]);

    float kscale = is_fp8 ? k_scales[kv_head_idx] : 1.0f;
    float vscale = is_fp8 ? v_scales[kv_head_idx] : 1.0f;

    device const uint32_t* seq_block_table = block_tables + seq_idx * max_blocks_per_seq;

    // Each thread processes assigned KV positions within this split
    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;
    float m_local = -INFINITY;
    float l_local = 0.0f;

    for (int kv_idx = split_start + thread_idx; kv_idx < split_end; kv_idx += NUM_THREADS) {
        int block_idx = kv_idx / BLOCK_SIZE;
        int block_offset = kv_idx % BLOCK_SIZE;
        uint32_t physical_block = seq_block_table[block_idx];

        device const cache_t* k_ptr = k_cache +
            (physical_block * BLOCK_SIZE + block_offset) * (num_kv_heads * HEAD_DIM) +
            kv_head_idx * HEAD_DIM;

        float dot = 0.0f;
        for (int d = 0; d < HEAD_DIM; d += 4) {
            float4 kv;
            if (is_fp8) {
                kv = float4(softmax_fp8_to_float(k_ptr[d]), softmax_fp8_to_float(k_ptr[d+1]),
                           softmax_fp8_to_float(k_ptr[d+2]), softmax_fp8_to_float(k_ptr[d+3])) * kscale;
            } else {
                kv = float4(float(k_ptr[d]), float(k_ptr[d+1]),
                           float(k_ptr[d+2]), float(k_ptr[d+3]));
            }
            dot += q_reg[d] * kv.x + q_reg[d+1] * kv.y +
                   q_reg[d+2] * kv.z + q_reg[d+3] * kv.w;
        }
        dot *= scale;
        if (softcapping > 0.0f) {
            dot = softcapping * precise::tanh(dot / softcapping);
        }

        float m_new = max(m_local, dot);
        float scale_old = exp(m_local - m_new);
        float p = exp(dot - m_new);
        for (int d = 0; d < HEAD_DIM; d++) o_acc[d] *= scale_old;
        l_local = l_local * scale_old + p;
        m_local = m_new;

        device const cache_t* v_ptr = v_cache +
            (physical_block * BLOCK_SIZE + block_offset) * (num_kv_heads * HEAD_DIM) +
            kv_head_idx * HEAD_DIM;

        for (int d = 0; d < HEAD_DIM; d += 4) {
            float4 vv;
            if (is_fp8) {
                vv = float4(softmax_fp8_to_float(v_ptr[d]), softmax_fp8_to_float(v_ptr[d+1]),
                           softmax_fp8_to_float(v_ptr[d+2]), softmax_fp8_to_float(v_ptr[d+3])) * vscale;
            } else {
                vv = float4(float(v_ptr[d]), float(v_ptr[d+1]),
                           float(v_ptr[d+2]), float(v_ptr[d+3]));
            }
            o_acc[d]   += p * vv.x;
            o_acc[d+1] += p * vv.y;
            o_acc[d+2] += p * vv.z;
            o_acc[d+3] += p * vv.w;
        }
    }

    // Reduce across threads in this threadgroup
    threadgroup float* s_max = (threadgroup float*)smem;
    threadgroup float* s_sum = s_max + NUM_THREADS;
    threadgroup float* s_reduce = s_sum + NUM_THREADS;

    // Reduce max
    s_max[thread_idx] = m_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < stride) {
            s_max[thread_idx] = max(s_max[thread_idx], s_max[thread_idx + stride]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_max = s_max[0];
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float my_scale2 = exp(m_local - global_max);
    l_local *= my_scale2;
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] *= my_scale2;

    // Reduce sum
    s_sum[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < stride) s_sum[thread_idx] += s_sum[thread_idx + stride];
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_sum_val = s_sum[0];
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Reduce output dimensions using simd_sum + cross-simdgroup merge
    constexpr int NUM_SIMD_GROUPS_SK = NUM_THREADS / 32;
    int out_base = ((seq_idx * num_q_heads + head_idx) * num_splits + split_idx) * HEAD_DIM;

    for (int d = 0; d < HEAD_DIM; d++) {
        o_acc[d] = simd_sum(o_acc[d]);
    }
    if (simd_lid == 0) {
        for (int d = 0; d < HEAD_DIM; d++) {
            s_reduce[simd_gid * HEAD_DIM + d] = o_acc[d];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (thread_idx < (uint)HEAD_DIM) {
        int d = thread_idx;
        float sum = 0.0f;
        for (int sg = 0; sg < NUM_SIMD_GROUPS_SK; sg++) {
            sum += s_reduce[sg * HEAD_DIM + d];
        }
        partial_out[out_base + d] = sum;
    }

    if (thread_idx == 0) {
        int meta_offset = (seq_idx * num_q_heads + head_idx) * num_splits + split_idx;
        partial_max[meta_offset] = global_max;
        partial_sum[meta_offset] = global_sum_val;
    }
}

// ============================================================================
// Split-K Reduce Kernel
// ============================================================================

template <typename T, int HEAD_DIM>
[[kernel]] void flash_attention_decode_reduce(
    device T* output [[buffer(0)]],
    device const float* partial_out [[buffer(1)]],
    device const float* partial_max [[buffer(2)]],
    device const float* partial_sum [[buffer(3)]],
    const constant int& num_q_heads [[buffer(4)]],
    const constant int& num_splits [[buffer(5)]],
    const constant int& q_stride [[buffer(6)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]]
) {
    const int head_idx = tgid.x;
    const int seq_idx = tgid.y;
    const int d = tid.x;

    if (d >= HEAD_DIM) return;

    int meta_base = (seq_idx * num_q_heads + head_idx) * num_splits;
    int out_base = meta_base * HEAD_DIM;

    // Find global max across splits
    float global_max = -INFINITY;
    for (int s = 0; s < num_splits; s++) {
        global_max = max(global_max, partial_max[meta_base + s]);
    }

    // Merge partial results
    float acc = 0.0f;
    float total_sum = 0.0f;
    for (int s = 0; s < num_splits; s++) {
        float s_max = partial_max[meta_base + s];
        float s_sum = partial_sum[meta_base + s];
        float rescale = exp(s_max - global_max);
        acc += partial_out[out_base + s * HEAD_DIM + d] * rescale;
        total_sum += s_sum * rescale;
    }

    device T* o_ptr = output + seq_idx * q_stride + head_idx * HEAD_DIM;
    o_ptr[d] = T(acc / max(total_sum, 1e-10f));
}

// ============================================================================
// Template Instantiations
// ============================================================================

#define INSTANTIATE_FLASH_PREFILL(T, CACHE_T, IS_FP8, HD, BS, BR, BC, SUFFIX) \
template [[host_name("flash_prefill_" SUFFIX)]] \
[[kernel]] void flash_attention_prefill<T, CACHE_T, IS_FP8, HD, BS, BR, BC>( \
    device T*, device const T*, device const CACHE_T*, device const CACHE_T*, \
    const constant int&, const constant float&, device const uint32_t*, \
    device const uint32_t*, const constant int&, const constant int&, \
    const constant int&, const constant int&, const constant float&, \
    const constant int&, device const uint32_t*, device const float*, \
    device const float*, device const float*, device const uint32_t*, \
    const constant int&, const constant int&, const constant int&, \
    const constant int&, \
    threadgroup char*, uint3, uint3, uint, uint);

#define INSTANTIATE_FLASH_DECODE(T, CACHE_T, IS_FP8, HD, BS, NT, SUFFIX) \
template [[host_name("flash_decode_" SUFFIX)]] \
[[kernel]] void flash_attention_decode<T, CACHE_T, IS_FP8, HD, BS, NT>( \
    device T*, device const T*, device const CACHE_T*, device const CACHE_T*, \
    device const uint32_t*, device const uint32_t*, \
    const constant int&, const constant int&, const constant int&, \
    const constant int&, const constant int&, const constant float&, \
    const constant int&, const constant int&, const constant float&, \
    const constant int&, device const float*, device const float*, \
    threadgroup char*, uint3, uint3, uint, uint);

#define INSTANTIATE_FLASH_DECODE_SPLITK(T, CACHE_T, IS_FP8, HD, BS, NT, SUFFIX) \
template [[host_name("flash_decode_splitk_" SUFFIX)]] \
[[kernel]] void flash_attention_decode_splitk<T, CACHE_T, IS_FP8, HD, BS, NT>( \
    device float*, device float*, device float*, \
    device const T*, device const CACHE_T*, device const CACHE_T*, \
    device const uint32_t*, device const uint32_t*, \
    const constant int&, const constant int&, const constant int&, \
    const constant int&, const constant int&, const constant float&, \
    const constant int&, const constant int&, const constant int&, \
    const constant float&, const constant int&, \
    device const float*, device const float*, \
    threadgroup char*, uint3, uint3, uint, uint);

#define INSTANTIATE_FLASH_REDUCE(T, HD, SUFFIX) \
template [[host_name("flash_decode_reduce_" SUFFIX)]] \
[[kernel]] void flash_attention_decode_reduce<T, HD>( \
    device T*, device const float*, device const float*, device const float*, \
    const constant int&, const constant int&, const constant int&, \
    uint3, uint3);

#define INSTANTIATE_FLASH_CACHE(T, CACHE_T, IS_FP8, SUFFIX) \
template [[host_name("flash_reshape_cache_" SUFFIX)]] \
[[kernel]] void flash_reshape_and_cache<T, CACHE_T, IS_FP8>( \
    device const T*, device const T*, device CACHE_T*, device CACHE_T*, \
    device const long*, const constant int&, const constant int&, \
    const constant int&, const constant int&, \
    const constant int&, const constant int&, \
    device const float*, device const float*, \
    uint3, uint3);

// BF16 model, BF16 cache, HEAD_DIM=128, BLOCK_SIZE=32
INSTANTIATE_FLASH_PREFILL(bfloat16_t, bfloat16_t, false, 128, 32, 64, 32, "bf16_hd128_bs32")
INSTANTIATE_FLASH_DECODE(bfloat16_t, bfloat16_t, false, 128, 32, 256, "bf16_hd128_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(bfloat16_t, bfloat16_t, false, 128, 32, 256, "bf16_hd128_bs32")
INSTANTIATE_FLASH_REDUCE(bfloat16_t, 128, "bf16_hd128")
INSTANTIATE_FLASH_CACHE(bfloat16_t, bfloat16_t, false, "bf16_bf16")

// F16 model, F16 cache
INSTANTIATE_FLASH_PREFILL(half, half, false, 128, 32, 64, 32, "f16_hd128_bs32")
INSTANTIATE_FLASH_DECODE(half, half, false, 128, 32, 256, "f16_hd128_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(half, half, false, 128, 32, 256, "f16_hd128_bs32")
INSTANTIATE_FLASH_REDUCE(half, 128, "f16_hd128")
INSTANTIATE_FLASH_CACHE(half, half, false, "f16_f16")

// BF16 model, FP8 (uint8_t) cache
INSTANTIATE_FLASH_PREFILL(bfloat16_t, uint8_t, true, 128, 32, 64, 32, "bf16_fp8_hd128_bs32")
INSTANTIATE_FLASH_DECODE(bfloat16_t, uint8_t, true, 128, 32, 256, "bf16_fp8_hd128_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(bfloat16_t, uint8_t, true, 128, 32, 256, "bf16_fp8_hd128_bs32")
INSTANTIATE_FLASH_CACHE(bfloat16_t, uint8_t, true, "bf16_fp8")

// F16 model, FP8 (uint8_t) cache
INSTANTIATE_FLASH_PREFILL(half, uint8_t, true, 128, 32, 64, 32, "f16_fp8_hd128_bs32")
INSTANTIATE_FLASH_DECODE(half, uint8_t, true, 128, 32, 256, "f16_fp8_hd128_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(half, uint8_t, true, 128, 32, 256, "f16_fp8_hd128_bs32")
INSTANTIATE_FLASH_CACHE(half, uint8_t, true, "f16_fp8")

// HEAD_DIM=256 variants (for Qwen3.5, etc.)
INSTANTIATE_FLASH_PREFILL(bfloat16_t, bfloat16_t, false, 256, 32, 32, 32, "bf16_hd256_bs32")
INSTANTIATE_FLASH_DECODE(bfloat16_t, bfloat16_t, false, 256, 32, 256, "bf16_hd256_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(bfloat16_t, bfloat16_t, false, 256, 32, 256, "bf16_hd256_bs32")
INSTANTIATE_FLASH_REDUCE(bfloat16_t, 256, "bf16_hd256")

INSTANTIATE_FLASH_PREFILL(half, half, false, 256, 32, 32, 32, "f16_hd256_bs32")
INSTANTIATE_FLASH_DECODE(half, half, false, 256, 32, 256, "f16_hd256_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(half, half, false, 256, 32, 256, "f16_hd256_bs32")
INSTANTIATE_FLASH_REDUCE(half, 256, "f16_hd256")

INSTANTIATE_FLASH_PREFILL(bfloat16_t, uint8_t, true, 256, 32, 32, 32, "bf16_fp8_hd256_bs32")
INSTANTIATE_FLASH_DECODE(bfloat16_t, uint8_t, true, 256, 32, 256, "bf16_fp8_hd256_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(bfloat16_t, uint8_t, true, 256, 32, 256, "bf16_fp8_hd256_bs32")

INSTANTIATE_FLASH_PREFILL(half, uint8_t, true, 256, 32, 32, 32, "f16_fp8_hd256_bs32")
INSTANTIATE_FLASH_DECODE(half, uint8_t, true, 256, 32, 256, "f16_fp8_hd256_bs32")
INSTANTIATE_FLASH_DECODE_SPLITK(half, uint8_t, true, 256, 32, 256, "f16_fp8_hd256_bs32")

// ============================================================================
// TurboQuant k8v4: FP8 keys + 4-bit values
// ============================================================================

// 4-bit quantization helpers
inline uint8_t tq_quantize_4bit(float val, float inv_absmax) {
    float scaled = val * inv_absmax * 7.5f + 7.5f;
    scaled = clamp(scaled, 0.0f, 15.0f);
    return (uint8_t)(scaled + 0.5f);
}

inline float tq_dequantize_4bit(uint8_t q, float absmax) {
    return ((float)q - 7.5f) / 7.5f * absmax;
}

inline uint8_t tq_pack_4bit(uint8_t lo, uint8_t hi) {
    return (hi << 4) | (lo & 0xF);
}

inline uint8_t tq_unpack_lo(uint8_t packed) { return packed & 0xF; }
inline uint8_t tq_unpack_hi(uint8_t packed) { return (packed >> 4) & 0xF; }

// TurboQuant k8v4 Store: Quantize V to 4-bit with per-head absmax
// K is already written to FP8 cache by flash_reshape_and_cache.
// This kernel only writes the 4-bit V and absmax metadata.
template <typename T, int HEAD_DIM>
[[kernel]] void flash_tq_store_k8v4(
    device const T* value [[buffer(0)]],          // [num_tokens, num_kv_heads, head_dim]
    device float* v_absmax [[buffer(1)]],          // [num_blocks, block_size, num_kv_heads]
    device uint8_t* v_quant [[buffer(2)]],         // [num_blocks, block_size, num_kv_heads, head_dim/2]
    device const long* slot_mapping [[buffer(3)]], // [num_tokens]
    const constant int& num_tokens [[buffer(4)]],
    const constant int& num_kv_heads [[buffer(5)]],
    const constant int& block_size [[buffer(6)]],
    uint3 tid [[thread_position_in_grid]],
    uint3 tpg [[threads_per_grid]]
) {
    int token_idx = tid.x;
    int head_idx = tid.y;
    if (token_idx >= num_tokens || head_idx >= num_kv_heads) return;

    long slot = slot_mapping[token_idx];
    if (slot < 0) return;

    int block_idx = (int)(slot / block_size);
    int block_off = (int)(slot % block_size);

    // Load V values and compute absmax
    int v_offset = token_idx * num_kv_heads * HEAD_DIM + head_idx * HEAD_DIM;
    float local_absmax = 0.0f;
    for (int d = 0; d < HEAD_DIM; d++) {
        float val = float(value[v_offset + d]);
        local_absmax = max(local_absmax, abs(val));
    }

    // Store absmax
    long am_off = (long)block_idx * block_size * num_kv_heads
        + (long)block_off * num_kv_heads + head_idx;
    v_absmax[am_off] = local_absmax;

    // Quantize and pack pairs
    float inv_absmax = (local_absmax > 0.0f) ? (1.0f / local_absmax) : 0.0f;
    long vq_off = (long)block_idx * block_size * num_kv_heads * (HEAD_DIM / 2)
        + (long)block_off * num_kv_heads * (HEAD_DIM / 2)
        + (long)head_idx * (HEAD_DIM / 2);

    for (int d = 0; d < HEAD_DIM; d += 2) {
        float v0 = float(value[v_offset + d]);
        float v1 = float(value[v_offset + d + 1]);
        uint8_t q0 = tq_quantize_4bit(v0, inv_absmax);
        uint8_t q1 = tq_quantize_4bit(v1, inv_absmax);
        v_quant[vq_off + d / 2] = tq_pack_4bit(q0, q1);
    }
}

// TurboQuant k8v4 Decode: FP8 keys + 4-bit values → attention output
// Uses same approach as flash_attention_decode but reads V from 4-bit packed buffers.
template <typename T, int HEAD_DIM, int BLOCK_SIZE, int NUM_THREADS>
[[kernel]] void flash_tq_decode_k8v4(
    device T* output [[buffer(0)]],
    device const T* q [[buffer(1)]],
    device const uint8_t* k_cache [[buffer(2)]],     // FP8 E4M3
    device const float* v_absmax [[buffer(3)]],      // [num_blocks, block_size, num_kv_heads]
    device const uint8_t* v_quant [[buffer(4)]],     // [num_blocks, block_size, num_kv_heads, head_dim/2]
    device const uint32_t* block_tables [[buffer(5)]],
    device const int32_t* context_lens [[buffer(6)]],
    device const float* k_scales [[buffer(7)]],
    const constant float& scale [[buffer(8)]],
    const constant float& softcapping [[buffer(9)]],
    const constant int& num_q_heads [[buffer(10)]],
    const constant int& num_kv_heads [[buffer(11)]],
    const constant int& block_size_param [[buffer(12)]],
    const constant int& max_context_len [[buffer(13)]],
    const constant int& num_seqs [[buffer(14)]],
    const constant int& head_dim_param [[buffer(15)]],
    const constant int& max_blocks_per_seq [[buffer(16)]],
    const constant int& q_stride [[buffer(17)]],
    const constant int& sliding_window [[buffer(18)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint lid [[thread_index_in_threadgroup]],
    uint simd_id [[simdgroup_index_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]]
) {
    const int seq_idx = tgid.x;
    const int head_idx = tgid.y;
    const int thread_idx = lid;
    if (seq_idx >= num_seqs) return;

    const int context_len = context_lens[seq_idx];
    if (context_len == 0) return;
    const int kv_head_idx = head_idx / (num_q_heads / num_kv_heads);

    // Load Q into shared memory
    threadgroup float q_shared[HEAD_DIM];
    device const T* q_ptr = q + seq_idx * q_stride + head_idx * HEAD_DIM;
    for (int d = thread_idx; d < HEAD_DIM; d += NUM_THREADS) {
        q_shared[d] = float(q_ptr[d]);
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float kscale = k_scales[kv_head_idx];

    device const uint32_t* seq_block_table = block_tables + seq_idx * max_blocks_per_seq;

    float m_local = -INFINITY;
    float l_local = 0.0f;
    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;

    int start_pos = 0;
    if (sliding_window > 0 && context_len > sliding_window) {
        start_pos = context_len - sliding_window;
    }

    for (int pos = start_pos + (int)thread_idx; pos < context_len; pos += NUM_THREADS) {
        int block_idx = pos / BLOCK_SIZE;
        int block_offset = pos % BLOCK_SIZE;
        int physical_block = (int)seq_block_table[block_idx];

        // K: read from FP8 cache with E4M3 dequant
        device const uint8_t* k_ptr = k_cache +
            (physical_block * BLOCK_SIZE + block_offset) * (num_kv_heads * HEAD_DIM) +
            kv_head_idx * HEAD_DIM;

        float dot = 0.0f;
        for (int d = 0; d < HEAD_DIM; d += 4) {
            float4 kv = float4(softmax_fp8_to_float(k_ptr[d]), softmax_fp8_to_float(k_ptr[d+1]),
                              softmax_fp8_to_float(k_ptr[d+2]), softmax_fp8_to_float(k_ptr[d+3])) * kscale;
            dot += q_shared[d] * kv.x + q_shared[d+1] * kv.y +
                   q_shared[d+2] * kv.z + q_shared[d+3] * kv.w;
        }
        dot *= scale;

        if (softcapping > 0.0f) {
            dot = softcapping * precise::tanh(dot / softcapping);
        }

        float m_new = max(m_local, dot);
        float scale_old = exp(m_local - m_new);
        float p = exp(dot - m_new);

        for (int d = 0; d < HEAD_DIM; d++) {
            o_acc[d] *= scale_old;
        }
        l_local = l_local * scale_old + p;
        m_local = m_new;

        // V: read from 4-bit packed with absmax dequant
        long am_off = (long)physical_block * BLOCK_SIZE * num_kv_heads
            + (long)block_offset * num_kv_heads + kv_head_idx;
        float absmax = v_absmax[am_off];

        long vq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * (HEAD_DIM / 2)
            + (long)block_offset * num_kv_heads * (HEAD_DIM / 2)
            + (long)kv_head_idx * (HEAD_DIM / 2);

        for (int d = 0; d < HEAD_DIM; d += 2) {
            uint8_t packed = v_quant[vq_off + d / 2];
            float v0 = tq_dequantize_4bit(tq_unpack_lo(packed), absmax);
            float v1 = tq_dequantize_4bit(tq_unpack_hi(packed), absmax);
            o_acc[d]   += p * v0;
            o_acc[d+1] += p * v1;
        }
    }

    // Cross-thread reduction (same strategy as proven flash_attention_decode)
    const int NUM_SIMD_GROUPS = NUM_THREADS / 32;
    threadgroup float smem[HEAD_DIM + NUM_THREADS * 2 + NUM_SIMD_GROUPS * HEAD_DIM];
    threadgroup float* s_m = smem;
    threadgroup float* s_l = s_m + NUM_THREADS;
    threadgroup float* s_reduce = s_l + NUM_THREADS;

    s_m[thread_idx] = m_local;
    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Tree reduction for global max
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) {
            s_m[thread_idx] = max(s_m[thread_idx], s_m[thread_idx + stride]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_max = s_m[0];
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Rescale each thread's l and o to global_max
    float my_rescale = exp(m_local - global_max);
    l_local *= my_rescale;
    for (int d = 0; d < HEAD_DIM; d++) {
        o_acc[d] *= my_rescale;
    }

    // Sum l values
    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) {
            s_l[thread_idx] += s_l[thread_idx + stride];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_l = s_l[0];
    float inv_l = 1.0f / max(global_l, 1e-10f);
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Reduce o_acc: simd_sum within simdgroups, then sum across simdgroups
    for (int d = 0; d < HEAD_DIM; d++) {
        o_acc[d] = simd_sum(o_acc[d]);
    }

    if (simd_lane == 0) {
        for (int d = 0; d < HEAD_DIM; d++) {
            s_reduce[simd_id * HEAD_DIM + d] = o_acc[d];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (thread_idx < (uint)HEAD_DIM) {
        int d = thread_idx;
        float sum = 0.0f;
        for (int sg = 0; sg < NUM_SIMD_GROUPS; sg++) {
            sum += s_reduce[sg * HEAD_DIM + d];
        }
        int out_off = seq_idx * q_stride + head_idx * HEAD_DIM + d;
        output[out_off] = T(sum * inv_l);
    }
}

// TQ k8v4 instantiations
#define INSTANTIATE_TQ_STORE(T, HD, NAME) \
    template [[host_name("flash_tq_store_k8v4_" NAME)]] \
    [[kernel]] void flash_tq_store_k8v4<T, HD>( \
        device const T*, device float*, device uint8_t*, \
        device const long*, const constant int&, const constant int&, \
        const constant int&, uint3, uint3);

#define INSTANTIATE_TQ_DECODE(T, HD, BS, NT, NAME) \
    template [[host_name("flash_tq_decode_k8v4_" NAME)]] \
    [[kernel]] void flash_tq_decode_k8v4<T, HD, BS, NT>( \
        device T*, device const T*, device const uint8_t*, \
        device const float*, device const uint8_t*, \
        device const uint32_t*, device const int32_t*, \
        device const float*, const constant float&, const constant float&, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant int&, const constant int&, \
        uint3, uint, uint, uint);

INSTANTIATE_TQ_STORE(bfloat16_t, 128, "bf16_hd128")
INSTANTIATE_TQ_STORE(half, 128, "f16_hd128")
INSTANTIATE_TQ_STORE(bfloat16_t, 256, "bf16_hd256")
INSTANTIATE_TQ_STORE(half, 256, "f16_hd256")

INSTANTIATE_TQ_DECODE(bfloat16_t, 128, 32, 256, "bf16_hd128_bs32")
INSTANTIATE_TQ_DECODE(half, 128, 32, 256, "f16_hd128_bs32")
INSTANTIATE_TQ_DECODE(bfloat16_t, 256, 32, 256, "bf16_hd256_bs32")
INSTANTIATE_TQ_DECODE(half, 256, 32, 256, "f16_hd256_bs32")

// ============================================================================
// TurboQuant Turbo4/Turbo3: WHT-rotated 4/3-bit keys + 4-bit values
// ============================================================================

// Deterministic sign flip: per-head, per-channel
inline float tq_sign_flip(uint head_idx, uint channel_idx) {
    uint hash = head_idx * 2654435761u + channel_idx * 40503u;
    return (hash & 1u) ? -1.0f : 1.0f;
}

// Walsh-Hadamard Transform helpers for simdgroups
// VEC = HEAD_DIM / 32 elements per thread
// Intra-thread butterfly on local VEC elements
template <int VEC>
inline void wht_intra(thread float* v) {
    for (int step = 1; step < VEC; step <<= 1) {
        for (int i = 0; i < VEC; i++) {
            int j = i ^ step;
            if (j > i) {
                float a = v[i], b = v[j];
                v[i] = a + b;
                v[j] = a - b;
            }
        }
    }
}

// Cross-thread butterfly stages via simd_shuffle_xor
template <int VEC>
inline void wht_cross(thread float* v, uint lane_id) {
    for (int stride = 1; stride < 32; stride <<= 1) {
        for (int i = 0; i < VEC; i++) {
            float other = simd_shuffle_xor(v[i], (ushort)stride);
            if (lane_id & stride)
                v[i] = other - v[i];
            else
                v[i] = v[i] + other;
        }
    }
}

// Full WHT = intra + cross + normalize
template <int VEC, int HDIM>
inline void wht_transform(thread float* v, uint lane_id) {
    wht_intra<VEC>(v);
    wht_cross<VEC>(v, lane_id);
    float norm = rsqrt((float)HDIM);
    for (int i = 0; i < VEC; i++) v[i] *= norm;
}

// ============================================================================
// Turbo4 Prefill Kernel
// ============================================================================
// Same structure as flash_attention_prefill but reads K/V from TQ4 buffers.
// Q is transformed with sign_flip + WHT to match stored K format.

template <typename T, int HEAD_DIM, int BLOCK_SIZE, int BR, int BC>
[[kernel]] void flash_tq4_prefill(
    device T* output [[buffer(0)]],
    device const T* q [[buffer(1)]],
    device const float* k_absmax [[buffer(2)]],
    device const uint8_t* k_quant [[buffer(3)]],
    device const float* v_absmax [[buffer(4)]],
    device const uint8_t* v_quant [[buffer(5)]],
    const constant int& num_kv_heads [[buffer(6)]],
    const constant float& scale [[buffer(7)]],
    device const uint32_t* block_tables [[buffer(8)]],
    device const uint32_t* seq_lens [[buffer(9)]],
    const constant int& block_table_stride [[buffer(10)]],
    const constant int& num_seqs [[buffer(11)]],
    const constant int& num_q_heads [[buffer(12)]],
    const constant int& num_q_tokens [[buffer(13)]],
    const constant float& softcapping [[buffer(14)]],
    const constant int& o_stride [[buffer(15)]],
    device const uint32_t* query_start_len [[buffer(16)]],
    const constant int& sliding_window [[buffer(17)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lid [[thread_index_in_simdgroup]]
) {
    const int q_head_within_kv = tgid.x;
    const int kv_head_idx = tgid.y;
    const int token_chunk_idx = tgid.z;

    const int num_queries_per_kv = num_q_heads / num_kv_heads;
    const int q_head_idx = kv_head_idx * num_queries_per_kv + q_head_within_kv;

    const int thread_idx = tid.x;

    const int token_start = token_chunk_idx * BR;
    const int my_token_idx = token_start + thread_idx;

    if (my_token_idx >= num_q_tokens) return;

    int seq_idx = 0;
    int query_start = 0;
    for (int s = 0; s < num_seqs; s++) {
        int next_start = query_start_len[s + 1];
        if (my_token_idx < (int)next_start) {
            seq_idx = s;
            break;
        }
        query_start = next_start;
    }

    const int local_q_idx = my_token_idx - query_start;
    const int context_len = seq_lens[seq_idx];
    const int q_len = query_start_len[seq_idx + 1] - query_start_len[seq_idx];
    const int kv_start = context_len - q_len;
    const int total_kv_len = context_len;

    // Load Q with sign_flip + WHT
    // For prefill each thread handles its own Q independently
    // WHT uses simdgroup cooperation, but in prefill each thread is one token
    // So we compute WHT locally (single-thread fallback for head_dim elements)
    float q_reg[HEAD_DIM];
    device const T* q_ptr = q + (my_token_idx * num_q_heads + q_head_idx) * HEAD_DIM;
    for (int d = 0; d < HEAD_DIM; d++) {
        q_reg[d] = float(q_ptr[d]) * tq_sign_flip(kv_head_idx, d);
    }
    // Single-thread Hadamard (butterfly) since one thread handles full HEAD_DIM
    for (int step = 1; step < HEAD_DIM; step <<= 1) {
        for (int i = 0; i < HEAD_DIM; i += step * 2) {
            for (int j = i; j < i + step; j++) {
                float a = q_reg[j];
                float b = q_reg[j + step];
                q_reg[j] = a + b;
                q_reg[j + step] = a - b;
            }
        }
    }
    float norm = rsqrt((float)HEAD_DIM);
    for (int d = 0; d < HEAD_DIM; d++) q_reg[d] *= norm;

    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;
    float m_prev = -INFINITY;
    float l_prev = 0.0f;

    device const uint32_t* seq_block_table = block_tables + seq_idx * block_table_stride;

    const int causal_end = kv_start + local_q_idx + 1;
    int kv_end = min(total_kv_len, causal_end);

    for (int kv_pos = 0; kv_pos < kv_end; kv_pos += BC) {
        int tile_end = min(kv_pos + BC, kv_end);
        int tile_len = tile_end - kv_pos;

        float m_cur = -INFINITY;
        float l_cur = 0.0f;
        float p_vals[BC];

        for (int t = 0; t < tile_len; t++) {
            int kv_idx = kv_pos + t;
            int block_idx = kv_idx / BLOCK_SIZE;
            int block_offset = kv_idx % BLOCK_SIZE;
            uint32_t physical_block = seq_block_table[block_idx];

            long kam_off = (long)physical_block * BLOCK_SIZE * num_kv_heads
                + (long)block_offset * num_kv_heads + kv_head_idx;
            float ka = k_absmax[kam_off];
            float ks = ka / 7.5f;

            long kq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * (HEAD_DIM / 2)
                + (long)block_offset * num_kv_heads * (HEAD_DIM / 2)
                + (long)kv_head_idx * (HEAD_DIM / 2);

            float dot = 0.0f;
            for (int d = 0; d < HEAD_DIM; d += 2) {
                uint8_t packed = k_quant[kq_off + d / 2];
                float k0 = ((float)(packed & 0xF) - 7.5f) * ks;
                float k1 = ((float)((packed >> 4) & 0xF) - 7.5f) * ks;
                dot += q_reg[d] * k0 + q_reg[d+1] * k1;
            }
            dot *= scale;

            if (softcapping > 0.0f) {
                dot = softcapping * precise::tanh(dot / softcapping);
            }
            if (kv_idx > kv_start + local_q_idx) {
                dot = -INFINITY;
            }

            p_vals[t] = dot;
            m_cur = max(m_cur, dot);
        }

        float m_new = max(m_prev, m_cur);
        float scale_prev = exp(m_prev - m_new);

        l_cur = 0.0f;
        for (int t = 0; t < tile_len; t++) {
            p_vals[t] = exp(p_vals[t] - m_new);
            l_cur += p_vals[t];
        }

        float l_new = l_prev * scale_prev + l_cur;

        for (int d = 0; d < HEAD_DIM; d++) {
            o_acc[d] *= (l_prev * scale_prev) / max(l_new, 1e-10f);
        }

        for (int t = 0; t < tile_len; t++) {
            int kv_idx = kv_pos + t;
            int block_idx = kv_idx / BLOCK_SIZE;
            int block_offset = kv_idx % BLOCK_SIZE;
            uint32_t physical_block = seq_block_table[block_idx];

            long vam_off = (long)physical_block * BLOCK_SIZE * num_kv_heads
                + (long)block_offset * num_kv_heads + kv_head_idx;
            float va = v_absmax[vam_off];
            float vs = va / 7.5f;

            long vq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * (HEAD_DIM / 2)
                + (long)block_offset * num_kv_heads * (HEAD_DIM / 2)
                + (long)kv_head_idx * (HEAD_DIM / 2);

            float weight = p_vals[t] / max(l_new, 1e-10f);
            for (int d = 0; d < HEAD_DIM; d += 2) {
                uint8_t packed = v_quant[vq_off + d / 2];
                float v0 = ((float)(packed & 0xF) - 7.5f) * vs;
                float v1 = ((float)((packed >> 4) & 0xF) - 7.5f) * vs;
                o_acc[d]   += weight * v0;
                o_acc[d+1] += weight * v1;
            }
        }

        m_prev = m_new;
        l_prev = l_new;
    }

    device T* o_ptr = output + (my_token_idx * num_q_heads + q_head_idx) * HEAD_DIM;
    for (int d = 0; d < HEAD_DIM; d++) {
        o_ptr[d] = T(o_acc[d]);
    }
}

// Turbo4 Store: K → sign_flip → WHT → 4-bit quant; V → 4-bit quant
template <typename T, int HEAD_DIM>
[[kernel]] void flash_tq4_store(
    device const T* key [[buffer(0)]],
    device const T* value [[buffer(1)]],
    device float* k_absmax [[buffer(2)]],
    device uint8_t* k_quant [[buffer(3)]],
    device float* v_absmax [[buffer(4)]],
    device uint8_t* v_quant [[buffer(5)]],
    device const long* slot_mapping [[buffer(6)]],
    const constant int& num_tokens [[buffer(7)]],
    const constant int& num_kv_heads [[buffer(8)]],
    const constant int& block_size [[buffer(9)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint lane_id [[thread_index_in_simdgroup]]
) {
    const int token_idx = tgid.x;
    const int head_idx = tgid.y;
    if (token_idx >= num_tokens || head_idx >= num_kv_heads) return;

    long slot = slot_mapping[token_idx];
    if (slot < 0) return;

    int block_idx = (int)(slot / block_size);
    int block_off = (int)(slot % block_size);

    constexpr int VEC = HEAD_DIM / 32;
    int base = token_idx * num_kv_heads * HEAD_DIM + head_idx * HEAD_DIM;
    int vec_off = lane_id * VEC;

    // --- K: sign_flip + WHT + 4-bit quant ---
    float k_reg[VEC];
    for (int i = 0; i < VEC; i++) {
        int ch = vec_off + i;
        k_reg[i] = float(key[base + ch]) * tq_sign_flip(head_idx, ch);
    }
    wht_transform<VEC, HEAD_DIM>(k_reg, lane_id);

    // Per-head absmax (simd reduction)
    float k_amax = 0.0f;
    for (int i = 0; i < VEC; i++) k_amax = max(k_amax, abs(k_reg[i]));
    k_amax = simd_max(k_amax);

    long am_off = (long)block_idx * block_size * num_kv_heads
        + (long)block_off * num_kv_heads + head_idx;
    if (lane_id == 0) k_absmax[am_off] = k_amax;
    k_amax = simd_broadcast(k_amax, 0);

    float k_inv_amax = (k_amax > 0.0f) ? (1.0f / k_amax) : 0.0f;
    long kq_off = (long)block_idx * block_size * num_kv_heads * (HEAD_DIM / 2)
        + (long)block_off * num_kv_heads * (HEAD_DIM / 2)
        + (long)head_idx * (HEAD_DIM / 2);

    for (int i = 0; i < VEC; i += 2) {
        uint8_t q0 = tq_quantize_4bit(k_reg[i], k_inv_amax);
        uint8_t q1 = tq_quantize_4bit(k_reg[i+1], k_inv_amax);
        int byte_idx = (vec_off + i) / 2;
        k_quant[kq_off + byte_idx] = tq_pack_4bit(q0, q1);
    }

    // --- V: 4-bit quant (no WHT) ---
    float v_reg[VEC];
    for (int i = 0; i < VEC; i++) {
        v_reg[i] = float(value[base + vec_off + i]);
    }

    float v_amax = 0.0f;
    for (int i = 0; i < VEC; i++) v_amax = max(v_amax, abs(v_reg[i]));
    v_amax = simd_max(v_amax);

    if (lane_id == 0) v_absmax[am_off] = v_amax;
    v_amax = simd_broadcast(v_amax, 0);

    float v_inv_amax = (v_amax > 0.0f) ? (1.0f / v_amax) : 0.0f;
    long vq_off = (long)block_idx * block_size * num_kv_heads * (HEAD_DIM / 2)
        + (long)block_off * num_kv_heads * (HEAD_DIM / 2)
        + (long)head_idx * (HEAD_DIM / 2);

    for (int i = 0; i < VEC; i += 2) {
        uint8_t q0 = tq_quantize_4bit(v_reg[i], v_inv_amax);
        uint8_t q1 = tq_quantize_4bit(v_reg[i+1], v_inv_amax);
        int byte_idx = (vec_off + i) / 2;
        v_quant[vq_off + byte_idx] = tq_pack_4bit(q0, q1);
    }
}

// Turbo4 Decode: 4-bit K (WHT-rotated) + 4-bit V → attention output
// Structured identically to flash_tq_decode_k8v4 (proven working) but reads K from k_quant/k_absmax
template <typename T, int HEAD_DIM, int BLOCK_SIZE, int NUM_THREADS>
[[kernel]] void flash_tq4_decode(
    device T* output [[buffer(0)]],
    device const T* q [[buffer(1)]],
    device const float* k_absmax [[buffer(2)]],
    device const uint8_t* k_quant [[buffer(3)]],
    device const float* v_absmax [[buffer(4)]],
    device const uint8_t* v_quant [[buffer(5)]],
    device const uint32_t* block_tables [[buffer(6)]],
    device const int32_t* context_lens [[buffer(7)]],
    const constant float& scale [[buffer(8)]],
    const constant float& softcapping [[buffer(9)]],
    const constant int& num_q_heads [[buffer(10)]],
    const constant int& num_kv_heads [[buffer(11)]],
    const constant int& block_size_param [[buffer(12)]],
    const constant int& num_seqs [[buffer(13)]],
    const constant int& head_dim_param [[buffer(14)]],
    const constant int& max_blocks_per_seq [[buffer(15)]],
    const constant int& q_stride [[buffer(16)]],
    const constant int& sliding_window [[buffer(17)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint lid [[thread_index_in_threadgroup]],
    uint simd_id [[simdgroup_index_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]]
) {
    const int seq_idx = tgid.x;
    const int head_idx = tgid.y;
    const int thread_idx = lid;
    if (seq_idx >= num_seqs) return;

    const int context_len = context_lens[seq_idx];
    if (context_len == 0) return;
    const int kv_head_idx = head_idx / (num_q_heads / num_kv_heads);

    constexpr int VEC = HEAD_DIM / 32;

    // Load Q with sign_flip + WHT into shared memory (simdgroup 0)
    threadgroup float q_shared[HEAD_DIM];
    if (simd_id == 0) {
        device const T* q_ptr = q + seq_idx * q_stride + head_idx * HEAD_DIM;
        int vec_off = simd_lane * VEC;
        float q_reg[VEC];
        for (int i = 0; i < VEC; i++) {
            int ch = vec_off + i;
            q_reg[i] = float(q_ptr[ch]) * tq_sign_flip(kv_head_idx, ch);
        }
        wht_transform<VEC, HEAD_DIM>(q_reg, simd_lane);
        for (int i = 0; i < VEC; i++) {
            q_shared[vec_off + i] = q_reg[i];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    device const uint32_t* seq_block_table = block_tables + seq_idx * max_blocks_per_seq;

    float m_local = -INFINITY;
    float l_local = 0.0f;
    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;

    int start_pos = 0;
    if (sliding_window > 0 && context_len > sliding_window) {
        start_pos = context_len - sliding_window;
    }

    for (int pos = start_pos + (int)thread_idx; pos < context_len; pos += NUM_THREADS) {
        int block_idx_local = pos / BLOCK_SIZE;
        int block_offset = pos % BLOCK_SIZE;
        int physical_block = (int)seq_block_table[block_idx_local];

        // K: dequant 4-bit from k_quant with k_absmax
        long kam_off = (long)physical_block * BLOCK_SIZE * num_kv_heads
            + (long)block_offset * num_kv_heads + kv_head_idx;
        float ka = k_absmax[kam_off];
        float ks = ka / 7.5f;

        long kq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * (HEAD_DIM / 2)
            + (long)block_offset * num_kv_heads * (HEAD_DIM / 2)
            + (long)kv_head_idx * (HEAD_DIM / 2);

        float dot = 0.0f;
        for (int d = 0; d < HEAD_DIM; d += 2) {
            uint8_t packed = k_quant[kq_off + d / 2];
            float k0 = ((float)(packed & 0xF) - 7.5f) * ks;
            float k1 = ((float)((packed >> 4) & 0xF) - 7.5f) * ks;
            dot += q_shared[d] * k0 + q_shared[d+1] * k1;
        }
        dot *= scale;

        if (softcapping > 0.0f) {
            dot = softcapping * precise::tanh(dot / softcapping);
        }

        float m_new = max(m_local, dot);
        float scale_old = exp(m_local - m_new);
        float p = exp(dot - m_new);
        for (int d = 0; d < HEAD_DIM; d++) o_acc[d] *= scale_old;
        l_local = l_local * scale_old + p;
        m_local = m_new;

        // V: dequant 4-bit from v_quant with v_absmax
        float va = v_absmax[kam_off];
        float vs_v = va / 7.5f;

        long vq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * (HEAD_DIM / 2)
            + (long)block_offset * num_kv_heads * (HEAD_DIM / 2)
            + (long)kv_head_idx * (HEAD_DIM / 2);

        for (int d = 0; d < HEAD_DIM; d += 2) {
            uint8_t packed = v_quant[vq_off + d / 2];
            float v0 = ((float)(packed & 0xF) - 7.5f) * vs_v;
            float v1 = ((float)((packed >> 4) & 0xF) - 7.5f) * vs_v;
            o_acc[d]   += p * v0;
            o_acc[d+1] += p * v1;
        }
    }

    // Cross-thread reduction (same as flash_tq_decode_k8v4)
    const int NUM_SIMD_GROUPS = NUM_THREADS / 32;
    threadgroup float smem[HEAD_DIM + NUM_THREADS * 2 + NUM_SIMD_GROUPS * HEAD_DIM];
    threadgroup float* s_m = smem;
    threadgroup float* s_l = s_m + NUM_THREADS;
    threadgroup float* s_reduce = s_l + NUM_THREADS;

    s_m[thread_idx] = m_local;
    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) {
            s_m[thread_idx] = max(s_m[thread_idx], s_m[thread_idx + stride]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_max = s_m[0];
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float my_rescale = exp(m_local - global_max);
    l_local *= my_rescale;
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] *= my_rescale;

    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) s_l[thread_idx] += s_l[thread_idx + stride];
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_l = s_l[0];
    float inv_l = 1.0f / max(global_l, 1e-10f);
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = simd_sum(o_acc[d]);

    if (simd_lane == 0) {
        for (int d = 0; d < HEAD_DIM; d++) s_reduce[simd_id * HEAD_DIM + d] = o_acc[d];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (thread_idx < (uint)HEAD_DIM) {
        int d = thread_idx;
        float sum = 0.0f;
        for (int sg = 0; sg < NUM_SIMD_GROUPS; sg++) sum += s_reduce[sg * HEAD_DIM + d];
        int out_off = seq_idx * q_stride + head_idx * HEAD_DIM + d;
        output[out_off] = T(sum * inv_l);
    }
}

// ============================================================================
// TurboQuant Turbo3: 3-bit K + 4-bit V
// K_quant stride per head = HEAD_DIM * 3 / 8 bytes
// 3-bit packing: 8 channels → 3 bytes (24 bits = 8×3-bit values)
//   q = clamp(round(val / absmax * 3.0) + 3, 0, 6) → 3-bit unsigned [0,6]
//   dequant: val = (q - 3) * absmax / 3.0
// ============================================================================

inline uint8_t tq3_quantize(float val, float inv_absmax) {
    float scaled = val * inv_absmax * 3.0f;
    int q = (int)(scaled + 3.5f);
    return (uint8_t)clamp(q, 0, 6);
}

inline void tq3_pack_8(const thread uint8_t vals[8], thread uint8_t out[3]) {
    uint bits = 0;
    for (int i = 0; i < 8; i++) bits |= ((uint)vals[i]) << (i * 3);
    out[0] = (uint8_t)(bits & 0xFF);
    out[1] = (uint8_t)((bits >> 8) & 0xFF);
    out[2] = (uint8_t)((bits >> 16) & 0xFF);
}

inline void tq3_unpack_8(const device uint8_t in[3], thread uint8_t vals[8]) {
    uint bits = (uint)in[0] | ((uint)in[1] << 8) | ((uint)in[2] << 16);
    for (int i = 0; i < 8; i++) vals[i] = (uint8_t)((bits >> (i * 3)) & 0x7);
}

// Turbo3 Store: K → sign_flip → WHT → 3-bit quant; V → 4-bit quant
template <typename T, int HEAD_DIM>
[[kernel]] void flash_tq3_store(
    device const T* key [[buffer(0)]],
    device const T* value [[buffer(1)]],
    device float* k_absmax [[buffer(2)]],
    device uint8_t* k_quant [[buffer(3)]],
    device float* v_absmax [[buffer(4)]],
    device uint8_t* v_quant [[buffer(5)]],
    device const long* slot_mapping [[buffer(6)]],
    const constant int& num_tokens [[buffer(7)]],
    const constant int& num_kv_heads [[buffer(8)]],
    const constant int& block_size [[buffer(9)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint lane_id [[thread_index_in_simdgroup]]
) {
    const int token_idx = tgid.x;
    const int head_idx = tgid.y;
    if (token_idx >= num_tokens || head_idx >= num_kv_heads) return;

    long slot = slot_mapping[token_idx];
    if (slot < 0) return;

    int block_idx = (int)(slot / block_size);
    int block_off = (int)(slot % block_size);

    constexpr int VEC = HEAD_DIM / 32;
    constexpr int K_BYTES_PER_HEAD = HEAD_DIM * 3 / 8;
    int base = token_idx * num_kv_heads * HEAD_DIM + head_idx * HEAD_DIM;
    int vec_off = lane_id * VEC;

    // --- K: sign_flip + WHT + 3-bit quant ---
    float k_reg[VEC];
    for (int i = 0; i < VEC; i++) {
        int ch = vec_off + i;
        k_reg[i] = float(key[base + ch]) * tq_sign_flip(head_idx, ch);
    }
    wht_transform<VEC, HEAD_DIM>(k_reg, lane_id);

    float k_amax = 0.0f;
    for (int i = 0; i < VEC; i++) k_amax = max(k_amax, abs(k_reg[i]));
    k_amax = simd_max(k_amax);

    long am_off = (long)block_idx * block_size * num_kv_heads
        + (long)block_off * num_kv_heads + head_idx;
    if (lane_id == 0) k_absmax[am_off] = k_amax;
    k_amax = simd_broadcast(k_amax, 0);

    float k_inv_amax = (k_amax > 0.0f) ? (1.0f / k_amax) : 0.0f;
    long kq_off = (long)block_idx * block_size * num_kv_heads * K_BYTES_PER_HEAD
        + (long)block_off * num_kv_heads * K_BYTES_PER_HEAD
        + (long)head_idx * K_BYTES_PER_HEAD;

    // 3-bit pack: 8 channels → 3 bytes. VEC channels per lane, total HEAD_DIM channels.
    // Each lane quantizes its VEC values, then cooperates to pack groups of 8.
    // For simplicity: each lane writes its own 3-byte groups when VEC >= 8.
    // When VEC < 8 (HEAD_DIM < 256), lanes cooperate via simd_shuffle.
    if constexpr (VEC >= 8) {
        for (int g = 0; g < VEC; g += 8) {
            uint8_t q3[8], packed[3];
            for (int i = 0; i < 8; i++) q3[i] = tq3_quantize(k_reg[g + i], k_inv_amax);
            tq3_pack_8(q3, packed);
            int global_ch = vec_off + g;
            int byte_base = (global_ch / 8) * 3;
            k_quant[kq_off + byte_base + 0] = packed[0];
            k_quant[kq_off + byte_base + 1] = packed[1];
            k_quant[kq_off + byte_base + 2] = packed[2];
        }
    } else {
        // VEC == 4 (HEAD_DIM == 128): each lane has 4 values, pairs of lanes form 8-groups
        uint8_t q3_local[VEC];
        for (int i = 0; i < VEC; i++) q3_local[i] = tq3_quantize(k_reg[i], k_inv_amax);

        if ((lane_id & 1) == 0) {
            uint8_t q3_8[8], packed[3];
            for (int i = 0; i < VEC; i++) q3_8[i] = q3_local[i];
            for (int i = 0; i < VEC; i++) {
                q3_8[VEC + i] = (uint8_t)simd_shuffle(
                    (ushort)q3_local[i], (ushort)(lane_id + 1));
            }
            tq3_pack_8(q3_8, packed);
            int global_ch = vec_off;
            int byte_base = (global_ch / 8) * 3;
            k_quant[kq_off + byte_base + 0] = packed[0];
            k_quant[kq_off + byte_base + 1] = packed[1];
            k_quant[kq_off + byte_base + 2] = packed[2];
        } else {
            for (int i = 0; i < VEC; i++) {
                simd_shuffle((ushort)q3_local[i], (ushort)(lane_id - 1));
            }
        }
    }

    // --- V: 4-bit quant (same as TQ4, no WHT) ---
    float v_reg[VEC];
    for (int i = 0; i < VEC; i++) {
        v_reg[i] = float(value[base + vec_off + i]);
    }

    float v_amax = 0.0f;
    for (int i = 0; i < VEC; i++) v_amax = max(v_amax, abs(v_reg[i]));
    v_amax = simd_max(v_amax);

    if (lane_id == 0) v_absmax[am_off] = v_amax;
    v_amax = simd_broadcast(v_amax, 0);

    float v_inv_amax = (v_amax > 0.0f) ? (1.0f / v_amax) : 0.0f;
    long vq_off = (long)block_idx * block_size * num_kv_heads * (HEAD_DIM / 2)
        + (long)block_off * num_kv_heads * (HEAD_DIM / 2)
        + (long)head_idx * (HEAD_DIM / 2);

    for (int i = 0; i < VEC; i += 2) {
        uint8_t q0 = tq_quantize_4bit(v_reg[i], v_inv_amax);
        uint8_t q1 = tq_quantize_4bit(v_reg[i+1], v_inv_amax);
        int byte_idx = (vec_off + i) / 2;
        v_quant[vq_off + byte_idx] = tq_pack_4bit(q0, q1);
    }
}

// Turbo3 Decode: 3-bit K (WHT-rotated) + 4-bit V → attention output
template <typename T, int HEAD_DIM, int BLOCK_SIZE, int NUM_THREADS>
[[kernel]] void flash_tq3_decode(
    device T* output [[buffer(0)]],
    device const T* q [[buffer(1)]],
    device const float* k_absmax [[buffer(2)]],
    device const uint8_t* k_quant [[buffer(3)]],
    device const float* v_absmax [[buffer(4)]],
    device const uint8_t* v_quant [[buffer(5)]],
    device const uint32_t* block_tables [[buffer(6)]],
    device const int32_t* context_lens [[buffer(7)]],
    const constant float& scale [[buffer(8)]],
    const constant float& softcapping [[buffer(9)]],
    const constant int& num_q_heads [[buffer(10)]],
    const constant int& num_kv_heads [[buffer(11)]],
    const constant int& block_size_param [[buffer(12)]],
    const constant int& num_seqs [[buffer(13)]],
    const constant int& head_dim_param [[buffer(14)]],
    const constant int& max_blocks_per_seq [[buffer(15)]],
    const constant int& q_stride [[buffer(16)]],
    const constant int& sliding_window [[buffer(17)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint lid [[thread_index_in_threadgroup]],
    uint simd_id [[simdgroup_index_in_threadgroup]],
    uint simd_lane [[thread_index_in_simdgroup]]
) {
    const int seq_idx = tgid.x;
    const int head_idx = tgid.y;
    const int thread_idx = lid;
    if (seq_idx >= num_seqs) return;

    const int context_len = context_lens[seq_idx];
    if (context_len == 0) return;
    const int kv_head_idx = head_idx / (num_q_heads / num_kv_heads);

    constexpr int VEC = HEAD_DIM / 32;
    constexpr int K_BYTES_PER_HEAD = HEAD_DIM * 3 / 8;

    threadgroup float q_shared[HEAD_DIM];
    if (simd_id == 0) {
        device const T* q_ptr = q + seq_idx * q_stride + head_idx * HEAD_DIM;
        int vec_off = simd_lane * VEC;
        float q_reg[VEC];
        for (int i = 0; i < VEC; i++) {
            int ch = vec_off + i;
            q_reg[i] = float(q_ptr[ch]) * tq_sign_flip(kv_head_idx, ch);
        }
        wht_transform<VEC, HEAD_DIM>(q_reg, simd_lane);
        for (int i = 0; i < VEC; i++) {
            q_shared[vec_off + i] = q_reg[i];
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    device const uint32_t* seq_block_table = block_tables + seq_idx * max_blocks_per_seq;

    float m_local = -INFINITY;
    float l_local = 0.0f;
    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;

    int start_pos = 0;
    if (sliding_window > 0 && context_len > sliding_window) {
        start_pos = context_len - sliding_window;
    }

    for (int pos = start_pos + (int)thread_idx; pos < context_len; pos += NUM_THREADS) {
        int block_idx_local = pos / BLOCK_SIZE;
        int block_offset = pos % BLOCK_SIZE;
        int physical_block = (int)seq_block_table[block_idx_local];

        // K: dequant 3-bit from k_quant with k_absmax
        long kam_off = (long)physical_block * BLOCK_SIZE * num_kv_heads
            + (long)block_offset * num_kv_heads + kv_head_idx;
        float ka = k_absmax[kam_off];
        float ks = ka / 3.0f;

        long kq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * K_BYTES_PER_HEAD
            + (long)block_offset * num_kv_heads * K_BYTES_PER_HEAD
            + (long)kv_head_idx * K_BYTES_PER_HEAD;

        float dot = 0.0f;
        for (int d = 0; d < HEAD_DIM; d += 8) {
            uint8_t q3[8];
            tq3_unpack_8(k_quant + kq_off + (d / 8) * 3, q3);
            for (int i = 0; i < 8; i++) {
                float k_val = ((float)q3[i] - 3.0f) * ks;
                dot += q_shared[d + i] * k_val;
            }
        }
        dot *= scale;

        if (softcapping > 0.0f) {
            dot = softcapping * precise::tanh(dot / softcapping);
        }

        float m_new = max(m_local, dot);
        float scale_old = exp(m_local - m_new);
        float p = exp(dot - m_new);
        for (int d = 0; d < HEAD_DIM; d++) o_acc[d] *= scale_old;
        l_local = l_local * scale_old + p;
        m_local = m_new;

        // V: dequant 4-bit (same as TQ4)
        float va = v_absmax[kam_off];
        float vs_v = va / 7.5f;

        long vq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * (HEAD_DIM / 2)
            + (long)block_offset * num_kv_heads * (HEAD_DIM / 2)
            + (long)kv_head_idx * (HEAD_DIM / 2);

        for (int d = 0; d < HEAD_DIM; d += 2) {
            uint8_t packed = v_quant[vq_off + d / 2];
            float v0 = ((float)(packed & 0xF) - 7.5f) * vs_v;
            float v1 = ((float)((packed >> 4) & 0xF) - 7.5f) * vs_v;
            o_acc[d]   += p * v0;
            o_acc[d+1] += p * v1;
        }
    }

    // Cross-thread reduction (same as TQ4 decode)
    const int NUM_SIMD_GROUPS = NUM_THREADS / 32;
    threadgroup float smem[HEAD_DIM + NUM_THREADS * 2 + NUM_SIMD_GROUPS * HEAD_DIM];
    threadgroup float* s_m = smem;
    threadgroup float* s_l = s_m + NUM_THREADS;
    threadgroup float* s_reduce = s_l + NUM_THREADS;

    s_m[thread_idx] = m_local;
    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) {
            s_m[thread_idx] = max(s_m[thread_idx], s_m[thread_idx + stride]);
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_max = s_m[0];
    threadgroup_barrier(mem_flags::mem_threadgroup);

    float my_rescale = exp(m_local - global_max);
    l_local *= my_rescale;
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] *= my_rescale;

    s_l[thread_idx] = l_local;
    threadgroup_barrier(mem_flags::mem_threadgroup);
    for (int stride = NUM_THREADS / 2; stride > 0; stride >>= 1) {
        if (thread_idx < (uint)stride) s_l[thread_idx] += s_l[thread_idx + stride];
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    float global_l = s_l[0];
    float inv_l = 1.0f / max(global_l, 1e-10f);
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = simd_sum(o_acc[d]);

    if (simd_lane == 0) {
        for (int d = 0; d < HEAD_DIM; d++) s_reduce[simd_id * HEAD_DIM + d] = o_acc[d];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (thread_idx < (uint)HEAD_DIM) {
        int d = thread_idx;
        float sum = 0.0f;
        for (int sg = 0; sg < NUM_SIMD_GROUPS; sg++) sum += s_reduce[sg * HEAD_DIM + d];
        int out_off = seq_idx * q_stride + head_idx * HEAD_DIM + d;
        output[out_off] = T(sum * inv_l);
    }
}

// Turbo3 Prefill: 3-bit K + 4-bit V flash attention
template <typename T, int HEAD_DIM, int BLOCK_SIZE, int BR, int BC>
[[kernel]] void flash_tq3_prefill(
    device T* output [[buffer(0)]],
    device const T* q [[buffer(1)]],
    device const float* k_absmax [[buffer(2)]],
    device const uint8_t* k_quant [[buffer(3)]],
    device const float* v_absmax [[buffer(4)]],
    device const uint8_t* v_quant [[buffer(5)]],
    const constant int& num_kv_heads [[buffer(6)]],
    const constant float& scale [[buffer(7)]],
    device const uint32_t* block_tables [[buffer(8)]],
    device const uint32_t* seq_lens [[buffer(9)]],
    const constant int& block_table_stride [[buffer(10)]],
    const constant int& num_seqs [[buffer(11)]],
    const constant int& num_q_heads [[buffer(12)]],
    const constant int& num_q_tokens [[buffer(13)]],
    const constant float& softcapping [[buffer(14)]],
    const constant int& o_stride [[buffer(15)]],
    device const uint32_t* query_start_len [[buffer(16)]],
    const constant int& sliding_window [[buffer(17)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid [[thread_position_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lid [[thread_index_in_simdgroup]]
) {
    const int q_head_within_kv = tgid.x;
    const int kv_head_idx = tgid.y;
    const int token_chunk_idx = tgid.z;

    const int num_queries_per_kv = num_q_heads / num_kv_heads;
    const int q_head_idx = kv_head_idx * num_queries_per_kv + q_head_within_kv;

    const int thread_idx = tid.x;

    const int token_start = token_chunk_idx * BR;
    const int my_token_idx = token_start + thread_idx;

    if (my_token_idx >= num_q_tokens) return;

    int seq_idx = 0;
    int query_start = 0;
    for (int s = 0; s < num_seqs; s++) {
        int next_start = query_start_len[s + 1];
        if (my_token_idx < (int)next_start) {
            seq_idx = s;
            break;
        }
        query_start = next_start;
    }

    const int local_q_idx = my_token_idx - query_start;
    const int context_len = seq_lens[seq_idx];
    const int q_len = query_start_len[seq_idx + 1] - query_start_len[seq_idx];
    const int kv_start = context_len - q_len;
    const int total_kv_len = context_len;

    // Load Q with sign_flip + WHT (single-thread butterfly)
    float q_reg[HEAD_DIM];
    device const T* q_ptr = q + (my_token_idx * num_q_heads + q_head_idx) * HEAD_DIM;
    for (int d = 0; d < HEAD_DIM; d++) {
        q_reg[d] = float(q_ptr[d]) * tq_sign_flip(kv_head_idx, d);
    }
    for (int step = 1; step < HEAD_DIM; step <<= 1) {
        for (int i = 0; i < HEAD_DIM; i += step * 2) {
            for (int j = i; j < i + step; j++) {
                float a = q_reg[j];
                float b = q_reg[j + step];
                q_reg[j] = a + b;
                q_reg[j + step] = a - b;
            }
        }
    }
    float norm = rsqrt((float)HEAD_DIM);
    for (int d = 0; d < HEAD_DIM; d++) q_reg[d] *= norm;

    constexpr int K_BYTES_PER_HEAD = HEAD_DIM * 3 / 8;

    float o_acc[HEAD_DIM];
    for (int d = 0; d < HEAD_DIM; d++) o_acc[d] = 0.0f;
    float m_prev = -INFINITY;
    float l_prev = 0.0f;

    device const uint32_t* seq_block_table = block_tables + seq_idx * block_table_stride;

    const int causal_end = kv_start + local_q_idx + 1;
    int kv_end = min(total_kv_len, causal_end);

    for (int kv_pos = 0; kv_pos < kv_end; kv_pos += BC) {
        int tile_end = min(kv_pos + BC, kv_end);
        int tile_len = tile_end - kv_pos;

        float m_cur = -INFINITY;
        float l_cur = 0.0f;
        float p_vals[BC];

        for (int t = 0; t < tile_len; t++) {
            int kv_idx = kv_pos + t;
            int block_idx = kv_idx / BLOCK_SIZE;
            int block_offset = kv_idx % BLOCK_SIZE;
            uint32_t physical_block = seq_block_table[block_idx];

            long kam_off = (long)physical_block * BLOCK_SIZE * num_kv_heads
                + (long)block_offset * num_kv_heads + kv_head_idx;
            float ka = k_absmax[kam_off];
            float ks = ka / 3.0f;

            long kq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * K_BYTES_PER_HEAD
                + (long)block_offset * num_kv_heads * K_BYTES_PER_HEAD
                + (long)kv_head_idx * K_BYTES_PER_HEAD;

            float dot = 0.0f;
            for (int d = 0; d < HEAD_DIM; d += 8) {
                uint8_t q3[8];
                tq3_unpack_8(k_quant + kq_off + (d / 8) * 3, q3);
                for (int i = 0; i < 8; i++) {
                    float k_val = ((float)q3[i] - 3.0f) * ks;
                    dot += q_reg[d + i] * k_val;
                }
            }
            dot *= scale;

            if (softcapping > 0.0f) {
                dot = softcapping * precise::tanh(dot / softcapping);
            }
            if (kv_idx > kv_start + local_q_idx) {
                dot = -INFINITY;
            }

            p_vals[t] = dot;
            m_cur = max(m_cur, dot);
        }

        float m_new = max(m_prev, m_cur);
        float scale_prev = exp(m_prev - m_new);

        l_cur = 0.0f;
        for (int t = 0; t < tile_len; t++) {
            p_vals[t] = exp(p_vals[t] - m_new);
            l_cur += p_vals[t];
        }

        float l_new = l_prev * scale_prev + l_cur;

        for (int d = 0; d < HEAD_DIM; d++) {
            o_acc[d] *= (l_prev * scale_prev) / max(l_new, 1e-10f);
        }

        for (int t = 0; t < tile_len; t++) {
            int kv_idx = kv_pos + t;
            int block_idx = kv_idx / BLOCK_SIZE;
            int block_offset = kv_idx % BLOCK_SIZE;
            uint32_t physical_block = seq_block_table[block_idx];

            long vam_off = (long)physical_block * BLOCK_SIZE * num_kv_heads
                + (long)block_offset * num_kv_heads + kv_head_idx;
            float va = v_absmax[vam_off];
            float vs = va / 7.5f;

            long vq_off = (long)physical_block * BLOCK_SIZE * num_kv_heads * (HEAD_DIM / 2)
                + (long)block_offset * num_kv_heads * (HEAD_DIM / 2)
                + (long)kv_head_idx * (HEAD_DIM / 2);

            float weight = p_vals[t] / max(l_new, 1e-10f);
            for (int d = 0; d < HEAD_DIM; d += 2) {
                uint8_t packed = v_quant[vq_off + d / 2];
                float v0 = ((float)(packed & 0xF) - 7.5f) * vs;
                float v1 = ((float)((packed >> 4) & 0xF) - 7.5f) * vs;
                o_acc[d]   += weight * v0;
                o_acc[d+1] += weight * v1;
            }
        }

        m_prev = m_new;
        l_prev = l_new;
    }

    device T* o_ptr = output + (my_token_idx * num_q_heads + q_head_idx) * HEAD_DIM;
    for (int d = 0; d < HEAD_DIM; d++) {
        o_ptr[d] = T(o_acc[d]);
    }
}

// Turbo3 instantiations
#define INSTANTIATE_TQ3_STORE(T, HD, NAME) \
    template [[host_name("flash_tq3_store_" NAME)]] \
    [[kernel]] void flash_tq3_store<T, HD>( \
        device const T*, device const T*, device float*, device uint8_t*, \
        device float*, device uint8_t*, device const long*, \
        const constant int&, const constant int&, const constant int&, \
        uint3, uint);

#define INSTANTIATE_TQ3_DECODE(T, HD, BS, NT, NAME) \
    template [[host_name("flash_tq3_decode_" NAME)]] \
    [[kernel]] void flash_tq3_decode<T, HD, BS, NT>( \
        device T*, device const T*, device const float*, device const uint8_t*, \
        device const float*, device const uint8_t*, \
        device const uint32_t*, device const int32_t*, \
        const constant float&, const constant float&, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant int&, \
        uint3, uint, uint, uint);

#define INSTANTIATE_TQ3_PREFILL(T, HD, BS, BR, BC, NAME) \
    template [[host_name("flash_tq3_prefill_" NAME)]] \
    [[kernel]] void flash_tq3_prefill<T, HD, BS, BR, BC>( \
        device T*, device const T*, device const float*, device const uint8_t*, \
        device const float*, device const uint8_t*, \
        const constant int&, const constant float&, \
        device const uint32_t*, device const uint32_t*, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant float&, const constant int&, \
        device const uint32_t*, const constant int&, \
        uint3, uint3, uint, uint);

INSTANTIATE_TQ3_STORE(bfloat16_t, 128, "bf16_hd128")
INSTANTIATE_TQ3_STORE(half, 128, "f16_hd128")
INSTANTIATE_TQ3_STORE(bfloat16_t, 256, "bf16_hd256")
INSTANTIATE_TQ3_STORE(half, 256, "f16_hd256")

INSTANTIATE_TQ3_DECODE(bfloat16_t, 128, 32, 256, "bf16_hd128_bs32")
INSTANTIATE_TQ3_DECODE(half, 128, 32, 256, "f16_hd128_bs32")
INSTANTIATE_TQ3_DECODE(bfloat16_t, 256, 32, 256, "bf16_hd256_bs32")
INSTANTIATE_TQ3_DECODE(half, 256, 32, 256, "f16_hd256_bs32")

INSTANTIATE_TQ3_PREFILL(bfloat16_t, 128, 32, 8, 32, "bf16_hd128_bs32")
INSTANTIATE_TQ3_PREFILL(half, 128, 32, 8, 32, "f16_hd128_bs32")
INSTANTIATE_TQ3_PREFILL(bfloat16_t, 256, 32, 8, 32, "bf16_hd256_bs32")
INSTANTIATE_TQ3_PREFILL(half, 256, 32, 8, 32, "f16_hd256_bs32")

// Turbo4 instantiations
#define INSTANTIATE_TQ4_STORE(T, HD, NAME) \
    template [[host_name("flash_tq4_store_" NAME)]] \
    [[kernel]] void flash_tq4_store<T, HD>( \
        device const T*, device const T*, device float*, device uint8_t*, \
        device float*, device uint8_t*, device const long*, \
        const constant int&, const constant int&, const constant int&, \
        uint3, uint);

#define INSTANTIATE_TQ4_DECODE(T, HD, BS, NT, NAME) \
    template [[host_name("flash_tq4_decode_" NAME)]] \
    [[kernel]] void flash_tq4_decode<T, HD, BS, NT>( \
        device T*, device const T*, device const float*, device const uint8_t*, \
        device const float*, device const uint8_t*, \
        device const uint32_t*, device const int32_t*, \
        const constant float&, const constant float&, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant int&, \
        uint3, uint, uint, uint);

INSTANTIATE_TQ4_STORE(bfloat16_t, 128, "bf16_hd128")
INSTANTIATE_TQ4_STORE(half, 128, "f16_hd128")
INSTANTIATE_TQ4_STORE(bfloat16_t, 256, "bf16_hd256")
INSTANTIATE_TQ4_STORE(half, 256, "f16_hd256")

INSTANTIATE_TQ4_DECODE(bfloat16_t, 128, 32, 256, "bf16_hd128_bs32")
INSTANTIATE_TQ4_DECODE(half, 128, 32, 256, "f16_hd128_bs32")
INSTANTIATE_TQ4_DECODE(bfloat16_t, 256, 32, 256, "bf16_hd256_bs32")
INSTANTIATE_TQ4_DECODE(half, 256, 32, 256, "f16_hd256_bs32")

#define INSTANTIATE_TQ4_PREFILL(T, HD, BS, BR, BC, NAME) \
    template [[host_name("flash_tq4_prefill_" NAME)]] \
    [[kernel]] void flash_tq4_prefill<T, HD, BS, BR, BC>( \
        device T*, device const T*, device const float*, device const uint8_t*, \
        device const float*, device const uint8_t*, \
        const constant int&, const constant float&, \
        device const uint32_t*, device const uint32_t*, \
        const constant int&, const constant int&, const constant int&, \
        const constant int&, const constant float&, const constant int&, \
        device const uint32_t*, const constant int&, \
        uint3, uint3, uint, uint);

INSTANTIATE_TQ4_PREFILL(bfloat16_t, 128, 32, 8, 32, "bf16_hd128_bs32")
INSTANTIATE_TQ4_PREFILL(half, 128, 32, 8, 32, "f16_hd128_bs32")
INSTANTIATE_TQ4_PREFILL(bfloat16_t, 256, 32, 8, 32, "bf16_hd256_bs32")
INSTANTIATE_TQ4_PREFILL(half, 256, 32, 8, 32, "f16_hd256_bs32")
