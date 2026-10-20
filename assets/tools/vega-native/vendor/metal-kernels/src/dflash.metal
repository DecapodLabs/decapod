#include "metal_dtype.metal"
#include <metal_stdlib>

using namespace metal;

static constant uint DFLASH_THREADS = 256;

// Iterative top-k over each row. This mirrors the CUDA DFlash path and writes
// both selected scores and U32 indices in descending score order.
kernel void dflash_topk_select_float(
    const device float* scores [[buffer(0)]],
    device float* topk_weights [[buffer(1)]],
    device uint* topk_indices [[buffer(2)]],
    constant uint& num_rows [[buffer(3)]],
    constant uint& num_experts [[buffer(4)]],
    constant uint& topk [[buffer(5)]],
    uint tid [[thread_index_in_threadgroup]],
    uint row [[threadgroup_position_in_grid]]) {
    if (row >= num_rows || topk == 0 || topk > 32) {
        return;
    }

    threadgroup float best_scores[DFLASH_THREADS];
    threadgroup uint best_indices[DFLASH_THREADS];
    threadgroup uint selected_indices[32];
    const uint row_offset = row * num_experts;
    const uint output_offset = row * topk;

    for (uint route = 0; route < topk; ++route) {
        float local_best = -INFINITY;
        uint local_index = 0xffffffffu;

        for (uint expert = tid; expert < num_experts; expert += DFLASH_THREADS) {
            bool already_selected = false;
            for (uint prior = 0; prior < route; ++prior) {
                if (selected_indices[prior] == expert) {
                    already_selected = true;
                    break;
                }
            }
            if (already_selected) {
                continue;
            }

            const float value = scores[row_offset + expert];
            if (value > local_best ||
                (value == local_best && expert < local_index)) {
                local_best = value;
                local_index = expert;
            }
        }

        best_scores[tid] = local_best;
        best_indices[tid] = local_index;
        threadgroup_barrier(mem_flags::mem_threadgroup);

        for (uint stride = DFLASH_THREADS / 2; stride > 0; stride >>= 1) {
            if (tid < stride) {
                const float other_score = best_scores[tid + stride];
                const uint other_index = best_indices[tid + stride];
                if (other_score > best_scores[tid] ||
                    (other_score == best_scores[tid] &&
                     other_index < best_indices[tid])) {
                    best_scores[tid] = other_score;
                    best_indices[tid] = other_index;
                }
            }
            threadgroup_barrier(mem_flags::mem_threadgroup);
        }

        if (tid == 0) {
            topk_weights[output_offset + route] = best_scores[0];
            topk_indices[output_offset + route] = best_indices[0];
            selected_indices[route] = best_indices[0];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
}

// Scores and walks the K-way candidate lattice. One threadgroup owns the full
// path because each selected token is the predecessor for the next position.
kernel void dflash_select_candidates_float(
    const device float* hidden [[buffer(0)]],
    const device float* unary_logits [[buffer(1)]],
    const device uint* candidate_ids [[buffer(2)]],
    const device float* predecessor_codebook [[buffer(3)]],
    const device float* successor_codebook [[buffer(4)]],
    const device uint* anchor_token [[buffer(5)]],
    device uint* selected_tokens [[buffer(6)]],
    constant uint& sequence_len [[buffer(7)]],
    constant uint& rank [[buffer(8)]],
    constant uint& topk [[buffer(9)]],
    uint tid [[thread_index_in_threadgroup]]) {
    if (topk == 0 || topk > 32) {
        return;
    }

    const uint lanes_per_candidate = DFLASH_THREADS / topk;
    const uint candidate = tid % topk;
    const uint lane = tid / topk;
    threadgroup float partial_dots[DFLASH_THREADS];
    threadgroup uint previous_token;

    if (tid == 0) {
        previous_token = anchor_token[0];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (uint position = 0; position < sequence_len; ++position) {
        if (lane < lanes_per_candidate) {
            const uint previous = previous_token;
            const uint candidate_token = candidate_ids[position * topk + candidate];
            const device float* h = hidden + position * rank;
            const device float* p = predecessor_codebook + previous * rank;
            const device float* s = successor_codebook + candidate_token * rank;

            float dot = 0.0f;
            for (uint r = lane; r < rank; r += lanes_per_candidate) {
                dot += h[r] * p[r] * s[r];
            }
            partial_dots[candidate * lanes_per_candidate + lane] = dot;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);

        if (tid == 0) {
            float best_score = -INFINITY;
            uint best_candidate = 0;
            for (uint candidate_idx = 0; candidate_idx < topk; ++candidate_idx) {
                float edge = 0.0f;
                for (uint lane_idx = 0; lane_idx < lanes_per_candidate; ++lane_idx) {
                    edge += partial_dots[candidate_idx * lanes_per_candidate + lane_idx];
                }
                const float score = unary_logits[position * topk + candidate_idx] + edge;
                if (score > best_score) {
                    best_score = score;
                    best_candidate = candidate_idx;
                }
            }
            const uint selected = candidate_ids[position * topk + best_candidate];
            selected_tokens[position] = selected;
            previous_token = selected;
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
}

template <typename T>
kernel void dflash_grouped_conv_kernel(
    const device T* hidden [[buffer(0)]],
    const device T* delta [[buffer(1)]],
    const device T* base_kernel [[buffer(2)]],
    device T* output [[buffer(3)]],
    constant uint& sequence_len [[buffer(4)]],
    constant uint& hidden_size [[buffer(5)]],
    constant uint& num_groups [[buffer(6)]],
    constant uint& group_size [[buffer(7)]],
    constant uint& taps [[buffer(8)]],
    constant uint& block_size [[buffer(9)]],
    constant uint& side [[buffer(10)]],
    uint index [[thread_position_in_grid]]) {
    const uint total = sequence_len * hidden_size;
    if (index >= total) {
        return;
    }

    const uint position = index / hidden_size;
    const uint channel = index % hidden_size;
    const uint group = channel / group_size;
    const uint local_position = position % block_size;
    float value = 0.0f;

    for (uint tap = 0; tap < taps; ++tap) {
        const float base = static_cast<float>(
            base_kernel[(side * taps + tap) * hidden_size + channel]);
        const float dynamic = static_cast<float>(
            delta[(position * taps + tap) * num_groups + group]);
        if (tap == 0 || local_position >= tap) {
            const uint source_position = position - tap;
            value += (base + dynamic) *
                static_cast<float>(hidden[source_position * hidden_size + channel]);
        }
    }
    output[index] = static_cast<T>(value);
}

#define INSTANTIATE_DFLASH_GROUPED_CONV(type, name) \
template [[host_name(name)]] \
[[kernel]] void dflash_grouped_conv_kernel<type>( \
    const device type* hidden [[buffer(0)]], \
    const device type* delta [[buffer(1)]], \
    const device type* base_kernel [[buffer(2)]], \
    device type* output [[buffer(3)]], \
    constant uint& sequence_len [[buffer(4)]], \
    constant uint& hidden_size [[buffer(5)]], \
    constant uint& num_groups [[buffer(6)]], \
    constant uint& group_size [[buffer(7)]], \
    constant uint& taps [[buffer(8)]], \
    constant uint& block_size [[buffer(9)]], \
    constant uint& side [[buffer(10)]], \
    uint index [[thread_position_in_grid]]);

INSTANTIATE_DFLASH_GROUPED_CONV(half, "dflash_grouped_conv_half")
INSTANTIATE_DFLASH_GROUPED_CONV(bfloat16_t, "dflash_grouped_conv_bfloat16_t")
