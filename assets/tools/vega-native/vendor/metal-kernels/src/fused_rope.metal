/**
 * @brief Fused Rotary Position Embedding (RoPE) Metal Kernels - With Position Selection
 * Copyright (c) 2025, Guoqingbao. All rights reserved.
 *
 * This kernel fuses TWO operations:
 *   1. Position-based cos/sin selection (eliminates index_select kernel)
 *   2. Rotary position embedding application
 *
 * Performance optimizations:
 *  - Native BF16/F32 compute, F16 -> F32 for precision (matches CUDA)
 *  - Vectorized pair operations using half2/Bfloat2_ (like CUDA's __half2/__bfloat162)
 *  - Single kernel for Q+K
 *  - GQA support (different Q/K head counts)
 *
 * Supports both interleaved and non-interleaved RoPE layouts.
 *
 * Licensed under the Apache License, Version 2.0
 */

#include "metal_dtype.metal"
#include <metal_stdlib>
using namespace metal;

constant uint ROPE_LAYOUT_BATCH_MAJOR = 0;
constant uint ROPE_LAYOUT_TOKEN_MAJOR = 1;

inline uint rope_interleaved_pair_index(
    uint local_idx,
    uint num_heads,
    uint seq_len,
    uint full_pairs,
    uint rotary_pairs,
    uint layout,
    thread uint& t_idx,
    thread uint& d_idx
) {
    if (layout == ROPE_LAYOUT_TOKEN_MAJOR) {
        const uint pairs_per_token = num_heads * rotary_pairs;
        t_idx = local_idx / pairs_per_token;
        const uint rem = local_idx % pairs_per_token;
        const uint h_idx = rem / rotary_pairs;
        d_idx = rem % rotary_pairs;
        return (t_idx * num_heads + h_idx) * full_pairs + d_idx;
    }

    const uint pairs_per_bh = seq_len * rotary_pairs;
    const uint bh_idx = local_idx / pairs_per_bh;
    const uint rem = local_idx % pairs_per_bh;
    t_idx = rem / rotary_pairs;
    d_idx = rem % rotary_pairs;
    return (bh_idx * seq_len + t_idx) * full_pairs + d_idx;
}

inline void rope_non_interleaved_indices(
    uint local_idx,
    uint num_heads,
    uint seq_len,
    uint d,
    uint rotary_pairs,
    uint layout,
    thread uint& t_idx,
    thread uint& i_d,
    thread uint& i1,
    thread uint& i2
) {
    uint base;

    if (layout == ROPE_LAYOUT_TOKEN_MAJOR) {
        const uint pairs_per_token = num_heads * rotary_pairs;
        t_idx = local_idx / pairs_per_token;
        const uint rem = local_idx % pairs_per_token;
        const uint h_idx = rem / rotary_pairs;
        i_d = rem % rotary_pairs;
        base = (t_idx * num_heads + h_idx) * d;
    } else {
        const uint pairs_per_bh = seq_len * rotary_pairs;
        const uint bh_idx = local_idx / pairs_per_bh;
        const uint rem = local_idx % pairs_per_bh;
        t_idx = rem / rotary_pairs;
        i_d = rem % rotary_pairs;
        base = (bh_idx * seq_len + t_idx) * d;
    }

    i1 = base + i_d;
    i2 = base + rotary_pairs + i_d;
}

// ============================================================================
// Interleaved RoPE with Position Selection
// Adjacent pairs: (x0, x1), (x2, x3), ...
// Uses vectorized access like CUDA's float2/half2/bfloat162
// ============================================================================

// F32 interleaved - uses float2 for vectorized pair access
kernel void fused_rope_i_f32(
    device float2* q [[buffer(0)]],      // Access as pairs
    device float2* k [[buffer(1)]],
    constant float* cos [[buffer(2)]],
    constant float* sin [[buffer(3)]],
    constant long* positions [[buffer(4)]],
    constant uint& q_bh [[buffer(5)]],
    constant uint& k_bh [[buffer(6)]],
    constant uint& seq_len [[buffer(7)]],
    constant uint& d [[buffer(8)]],
    constant uint& rotary_dim [[buffer(9)]],
    constant uint& layout [[buffer(10)]],
    uint idx [[thread_position_in_grid]]
) {
    const uint full_pairs = d / 2;
    const uint rotary_pairs = rotary_dim / 2;
    const uint q_num_pairs = q_bh * seq_len * rotary_pairs;
    const uint k_num_pairs = k_bh * seq_len * rotary_pairs;
    const uint total_pairs = q_num_pairs + k_num_pairs;
    
    if (idx >= total_pairs) return;
    
    const bool is_q = (idx < q_num_pairs);
    const uint local_idx = is_q ? idx : (idx - q_num_pairs);
    
    uint t_idx;
    uint d_idx;
    const uint pair_idx = rope_interleaved_pair_index(
        local_idx,
        is_q ? q_bh : k_bh,
        seq_len,
        full_pairs,
        rotary_pairs,
        layout,
        t_idx,
        d_idx
    );
    
    const long pos = positions[t_idx];
    const uint cs_idx = pos * rotary_pairs + d_idx;
    const float c = cos[cs_idx];
    const float s = sin[cs_idx];
    
    device float2* ptr = is_q ? q : k;
    float2 v = ptr[pair_idx];
    
    float2 result;
    result.x = v.x * c - v.y * s;
    result.y = v.x * s + v.y * c;
    
    ptr[pair_idx] = result;
}

// F16 interleaved - uses half2 for vectorized pair access, F32 compute for precision
kernel void fused_rope_i_f16(
    device half2* q [[buffer(0)]],
    device half2* k [[buffer(1)]],
    constant half* cos [[buffer(2)]],
    constant half* sin [[buffer(3)]],
    constant long* positions [[buffer(4)]],
    constant uint& q_bh [[buffer(5)]],
    constant uint& k_bh [[buffer(6)]],
    constant uint& seq_len [[buffer(7)]],
    constant uint& d [[buffer(8)]],
    constant uint& rotary_dim [[buffer(9)]],
    constant uint& layout [[buffer(10)]],
    uint idx [[thread_position_in_grid]]
) {
    const uint full_pairs = d / 2;
    const uint rotary_pairs = rotary_dim / 2;
    const uint q_num_pairs = q_bh * seq_len * rotary_pairs;
    const uint k_num_pairs = k_bh * seq_len * rotary_pairs;
    const uint total_pairs = q_num_pairs + k_num_pairs;
    
    if (idx >= total_pairs) return;
    
    const bool is_q = (idx < q_num_pairs);
    const uint local_idx = is_q ? idx : (idx - q_num_pairs);
    
    uint t_idx;
    uint d_idx;
    const uint pair_idx = rope_interleaved_pair_index(
        local_idx,
        is_q ? q_bh : k_bh,
        seq_len,
        full_pairs,
        rotary_pairs,
        layout,
        t_idx,
        d_idx
    );
    
    const long pos = positions[t_idx];
    const uint cs_idx = pos * rotary_pairs + d_idx;
    
    // F32 compute for precision (like CUDA)
    const float c = float(cos[cs_idx]);
    const float s = float(sin[cs_idx]);
    
    device half2* ptr = is_q ? q : k;
    half2 v = ptr[pair_idx];
    
    float vx = float(v.x);
    float vy = float(v.y);
    
    half2 result;
    result.x = half(vx * c - vy * s);
    result.y = half(vx * s + vy * c);
    
    ptr[pair_idx] = result;
}

// BF16 interleaved - uses Bfloat2_ for vectorized pair access, native BF16 compute
kernel void fused_rope_i_bf16(
    device Bfloat2_* q [[buffer(0)]],
    device Bfloat2_* k [[buffer(1)]],
    constant bfloat16_t* cos [[buffer(2)]],
    constant bfloat16_t* sin [[buffer(3)]],
    constant long* positions [[buffer(4)]],
    constant uint& q_bh [[buffer(5)]],
    constant uint& k_bh [[buffer(6)]],
    constant uint& seq_len [[buffer(7)]],
    constant uint& d [[buffer(8)]],
    constant uint& rotary_dim [[buffer(9)]],
    constant uint& layout [[buffer(10)]],
    uint idx [[thread_position_in_grid]]
) {
    const uint full_pairs = d / 2;
    const uint rotary_pairs = rotary_dim / 2;
    const uint q_num_pairs = q_bh * seq_len * rotary_pairs;
    const uint k_num_pairs = k_bh * seq_len * rotary_pairs;
    const uint total_pairs = q_num_pairs + k_num_pairs;
    
    if (idx >= total_pairs) return;
    
    const bool is_q = (idx < q_num_pairs);
    const uint local_idx = is_q ? idx : (idx - q_num_pairs);
    
    uint t_idx;
    uint d_idx;
    const uint pair_idx = rope_interleaved_pair_index(
        local_idx,
        is_q ? q_bh : k_bh,
        seq_len,
        full_pairs,
        rotary_pairs,
        layout,
        t_idx,
        d_idx
    );
    
    const long pos = positions[t_idx];
    const uint cs_idx = pos * rotary_pairs + d_idx;
    
    // Native BF16 compute (like CUDA's __hmul, __hsub, __hadd)
    const bfloat16_t c = cos[cs_idx];
    const bfloat16_t s = sin[cs_idx];
    
    device Bfloat2_* ptr = is_q ? q : k;
    Bfloat2_ v = ptr[pair_idx];
    
    Bfloat2_ result;
    result.x = v.x * c - v.y * s;
    result.y = v.x * s + v.y * c;
    
    ptr[pair_idx] = result;
}

// ============================================================================
// Non-Interleaved RoPE with Position Selection
// Split pairs: x[i] and x[i + d/2]
// ============================================================================

// F32 non-interleaved
kernel void fused_rope_f32(
    device float* q [[buffer(0)]],
    device float* k [[buffer(1)]],
    constant float* cos [[buffer(2)]],
    constant float* sin [[buffer(3)]],
    constant long* positions [[buffer(4)]],
    constant uint& q_bh [[buffer(5)]],
    constant uint& k_bh [[buffer(6)]],
    constant uint& seq_len [[buffer(7)]],
    constant uint& d [[buffer(8)]],
    constant uint& rotary_dim [[buffer(9)]],
    constant uint& layout [[buffer(10)]],
    uint idx [[thread_position_in_grid]]
) {
    const uint rotary_pairs = rotary_dim / 2;
    const uint q_pairs = q_bh * seq_len * rotary_pairs;
    const uint k_pairs = k_bh * seq_len * rotary_pairs;
    const uint total_pairs = q_pairs + k_pairs;
    
    if (idx >= total_pairs) return;
    
    const bool is_q = (idx < q_pairs);
    const uint local_idx = is_q ? idx : (idx - q_pairs);
    
    uint i_t;
    uint i_d;
    uint i1;
    uint i2;
    rope_non_interleaved_indices(
        local_idx,
        is_q ? q_bh : k_bh,
        seq_len,
        d,
        rotary_pairs,
        layout,
        i_t,
        i_d,
        i1,
        i2
    );
    
    const long pos = positions[i_t];
    const uint cs_idx = pos * rotary_pairs + i_d;
    const float c = cos[cs_idx];
    const float s = sin[cs_idx];
    
    device float* ptr = is_q ? q : k;
    float x1 = ptr[i1];
    float x2 = ptr[i2];
    
    // Simple scalar ops
    ptr[i1] = x1 * c - x2 * s;
    ptr[i2] = x1 * s + x2 * c;
}

// F16 non-interleaved - F32 compute for precision
kernel void fused_rope_f16(
    device half* q [[buffer(0)]],
    device half* k [[buffer(1)]],
    constant half* cos [[buffer(2)]],
    constant half* sin [[buffer(3)]],
    constant long* positions [[buffer(4)]],
    constant uint& q_bh [[buffer(5)]],
    constant uint& k_bh [[buffer(6)]],
    constant uint& seq_len [[buffer(7)]],
    constant uint& d [[buffer(8)]],
    constant uint& rotary_dim [[buffer(9)]],
    constant uint& layout [[buffer(10)]],
    uint idx [[thread_position_in_grid]]
) {
    const uint rotary_pairs = rotary_dim / 2;
    const uint q_pairs = q_bh * seq_len * rotary_pairs;
    const uint k_pairs = k_bh * seq_len * rotary_pairs;
    const uint total_pairs = q_pairs + k_pairs;
    
    if (idx >= total_pairs) return;
    
    const bool is_q = (idx < q_pairs);
    const uint local_idx = is_q ? idx : (idx - q_pairs);
    
    uint i_t;
    uint i_d;
    uint i1;
    uint i2;
    rope_non_interleaved_indices(
        local_idx,
        is_q ? q_bh : k_bh,
        seq_len,
        d,
        rotary_pairs,
        layout,
        i_t,
        i_d,
        i1,
        i2
    );
    
    const long pos = positions[i_t];
    const uint cs_idx = pos * rotary_pairs + i_d;
    
    // F32 compute for precision
    const float c = float(cos[cs_idx]);
    const float s = float(sin[cs_idx]);
    
    device half* ptr = is_q ? q : k;
    float x1 = float(ptr[i1]);
    float x2 = float(ptr[i2]);
    
    // Simple scalar ops with F32 precision
    ptr[i1] = half(x1 * c - x2 * s);
    ptr[i2] = half(x1 * s + x2 * c);
}

// BF16 non-interleaved - native BF16 compute
kernel void fused_rope_bf16(
    device bfloat16_t* q [[buffer(0)]],
    device bfloat16_t* k [[buffer(1)]],
    constant bfloat16_t* cos [[buffer(2)]],
    constant bfloat16_t* sin [[buffer(3)]],
    constant long* positions [[buffer(4)]],
    constant uint& q_bh [[buffer(5)]],
    constant uint& k_bh [[buffer(6)]],
    constant uint& seq_len [[buffer(7)]],
    constant uint& d [[buffer(8)]],
    constant uint& rotary_dim [[buffer(9)]],
    constant uint& layout [[buffer(10)]],
    uint idx [[thread_position_in_grid]]
) {
    const uint rotary_pairs = rotary_dim / 2;
    const uint q_pairs = q_bh * seq_len * rotary_pairs;
    const uint k_pairs = k_bh * seq_len * rotary_pairs;
    const uint total_pairs = q_pairs + k_pairs;
    
    if (idx >= total_pairs) return;
    
    const bool is_q = (idx < q_pairs);
    const uint local_idx = is_q ? idx : (idx - q_pairs);
    
    uint i_t;
    uint i_d;
    uint i1;
    uint i2;
    rope_non_interleaved_indices(
        local_idx,
        is_q ? q_bh : k_bh,
        seq_len,
        d,
        rotary_pairs,
        layout,
        i_t,
        i_d,
        i1,
        i2
    );
    
    const long pos = positions[i_t];
    const uint cs_idx = pos * rotary_pairs + i_d;
    
    // Native BF16 compute
    const bfloat16_t c = cos[cs_idx];
    const bfloat16_t s = sin[cs_idx];
    
    device bfloat16_t* ptr = is_q ? q : k;
    bfloat16_t x1 = ptr[i1];
    bfloat16_t x2 = ptr[i2];
    
    // Simple scalar ops with native BF16
    ptr[i1] = x1 * c - x2 * s;
    ptr[i2] = x1 * s + x2 * c;
}
