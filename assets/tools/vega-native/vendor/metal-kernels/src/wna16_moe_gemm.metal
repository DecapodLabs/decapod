#include "metal_dtype.metal"
#include <metal_stdlib>

using namespace metal;

// WNA16 (INT4/INT8 weight-only) MoE GEMM for Metal.
//
// Matches the CUDA moe_gemv_wna16 / moe_gemm_wmma_wna16 contract used by
// attention-rs::moe::moe_gemm_wna16:
//   output[token_id, n] = input[input_idx, :] @ W[expert, n, :]^T
// where
//   input_idx = has_topk_weights ? token_id : token_id / topk
//   token_id  = sorted_token_ids[m]
//   expert    = expert_ids[m]
//
// Weight format: U32 packed little-endian INT4/INT8, [E, N, K / pack_factor]
// Scale format:  F32 per-group, [E, N, K / group_size]
// Input/Output:  F16/BF16

#define WARP_SIZE 32

template <typename T, int BITS>
[[kernel]] void wna16_moe_gemv_kernel(
    device const T        *input            [[ buffer(0) ]],
    device const uint     *weights          [[ buffer(1) ]],
    device const float    *weight_scales    [[ buffer(2) ]],
    device const int      *sorted_token_ids [[ buffer(3) ]],
    device const int      *expert_ids       [[ buffer(4) ]],
    device const float    *topk_weights     [[ buffer(5) ]],
    device       T        *output           [[ buffer(6) ]],
    constant     int      &num_experts      [[ buffer(7) ]],
    constant     int      &topk             [[ buffer(8) ]],
    constant     int      &size_m           [[ buffer(9) ]],
    constant     int      &size_n           [[ buffer(10) ]],
    constant     int      &size_k           [[ buffer(11) ]],
    constant     int      &group_size       [[ buffer(12) ]],
    constant     int      &zero_point       [[ buffer(13) ]],
    constant     int      &has_topk_weights [[ buffer(14) ]],
    uint3 gid            [[ threadgroup_position_in_grid ]],
    uint  lane_id        [[ thread_index_in_simdgroup ]]
) {
    const int n_out = int(gid.x);
    const int m_out = int(gid.y);
    if (n_out >= size_n || m_out >= size_m) return;

    const int token_id = sorted_token_ids[m_out];
    const int expert_id = expert_ids[m_out];

    if (expert_id < 0 || expert_id >= num_experts) {
        return;
    }

    const int pack_factor = 32 / BITS;
    const int packed_k = size_k / pack_factor;
    const int scale_k = size_k / group_size;
    const uint mask = (1u << BITS) - 1u;

    const int input_idx = has_topk_weights != 0 ? token_id : token_id / topk;
    device const T *x_ptr = input + (long)input_idx * size_k;
    device const uint *w_ptr =
        weights + ((long)expert_id * size_n + n_out) * packed_k;
    device const float *s_ptr =
        weight_scales + ((long)expert_id * size_n + n_out) * scale_k;

    float sum_f = 0.0f;
    const bool one_scale_per_word =
        group_size >= pack_factor && (group_size % pack_factor) == 0;

    for (int packed_idx = int(lane_id); packed_idx < packed_k; packed_idx += WARP_SIZE) {
        const uint word = w_ptr[packed_idx];
        const int k_base = packed_idx * pack_factor;
        const float word_scale =
            one_scale_per_word ? s_ptr[k_base / group_size] : 0.0f;

        for (int q_idx = 0; q_idx < pack_factor; ++q_idx) {
            const int k = k_base + q_idx;
            if (k >= size_k) continue;
            const int q = int((word >> (q_idx * BITS)) & mask) - zero_point;
            const float scale =
                one_scale_per_word ? word_scale : s_ptr[k / group_size];
            float xf;
            if constexpr (is_same_v<T, bfloat16_t>) {
                xf = float(x_ptr[k]);
            } else {
                xf = float((half)x_ptr[k]);
            }
            sum_f += xf * (float(q) * scale);
        }
    }

    sum_f = simd_sum(sum_f);

    if (lane_id == 0) {
        if (has_topk_weights != 0) {
            sum_f *= topk_weights[token_id];
        }
        if constexpr (is_same_v<T, bfloat16_t>) {
            output[(long)token_id * size_n + n_out] = static_cast<bfloat16_t>(sum_f);
        } else {
            output[(long)token_id * size_n + n_out] = static_cast<T>(sum_f);
        }
    }
}

// Prefill / large-M path: each lane owns one N column and walks M rows in the
// threadgroup tile. Still scatters to token_id so results stay in original
// assignment order for the caller.
template <typename T, int BITS, int BLOCK_M, int BLOCK_N>
[[kernel]] void wna16_moe_gemm_kernel(
    device const T        *input            [[ buffer(0) ]],
    device const uint     *weights          [[ buffer(1) ]],
    device const float    *weight_scales    [[ buffer(2) ]],
    device const int      *sorted_token_ids [[ buffer(3) ]],
    device const int      *expert_ids       [[ buffer(4) ]],
    device const float    *topk_weights     [[ buffer(5) ]],
    device       T        *output           [[ buffer(6) ]],
    constant     int      &num_experts      [[ buffer(7) ]],
    constant     int      &topk             [[ buffer(8) ]],
    constant     int      &size_m           [[ buffer(9) ]],
    constant     int      &size_n           [[ buffer(10) ]],
    constant     int      &size_k           [[ buffer(11) ]],
    constant     int      &group_size       [[ buffer(12) ]],
    constant     int      &zero_point       [[ buffer(13) ]],
    constant     int      &has_topk_weights [[ buffer(14) ]],
    uint3 gid            [[ threadgroup_position_in_grid ]],
    uint  lane_id        [[ thread_index_in_simdgroup ]]
) {
    const int n_out = int(gid.x) * BLOCK_N + int(lane_id);
    const int m_block_base = int(gid.y) * BLOCK_M;
    if (n_out >= size_n) return;

    const int pack_factor = 32 / BITS;
    const int packed_k = size_k / pack_factor;
    const int scale_k = size_k / group_size;
    const uint mask = (1u << BITS) - 1u;
    const bool one_scale_per_word =
        group_size >= pack_factor && (group_size % pack_factor) == 0;

    for (int mb = 0; mb < BLOCK_M; ++mb) {
        const int m_out = m_block_base + mb;
        if (m_out >= size_m) break;

        const int token_id = sorted_token_ids[m_out];
        const int expert_id = expert_ids[m_out];

        if (expert_id < 0 || expert_id >= num_experts) {
            continue;
        }

        const int input_idx = has_topk_weights != 0 ? token_id : token_id / topk;
        device const T *x_ptr = input + (long)input_idx * size_k;
        device const uint *w_ptr =
            weights + ((long)expert_id * size_n + n_out) * packed_k;
        device const float *s_ptr =
            weight_scales + ((long)expert_id * size_n + n_out) * scale_k;

        float sum_f = 0.0f;
        for (int packed_idx = 0; packed_idx < packed_k; ++packed_idx) {
            const uint word = w_ptr[packed_idx];
            const int k_base = packed_idx * pack_factor;
            const float word_scale =
                one_scale_per_word ? s_ptr[k_base / group_size] : 0.0f;

            for (int q_idx = 0; q_idx < pack_factor; ++q_idx) {
                const int k = k_base + q_idx;
                if (k >= size_k) continue;
                const int q = int((word >> (q_idx * BITS)) & mask) - zero_point;
                const float scale =
                    one_scale_per_word ? word_scale : s_ptr[k / group_size];
                float xf;
                if constexpr (is_same_v<T, bfloat16_t>) {
                    xf = float(x_ptr[k]);
                } else {
                    xf = float((half)x_ptr[k]);
                }
                sum_f += xf * (float(q) * scale);
            }
        }

        if (has_topk_weights != 0) {
            sum_f *= topk_weights[token_id];
        }
        if constexpr (is_same_v<T, bfloat16_t>) {
            output[(long)token_id * size_n + n_out] = static_cast<bfloat16_t>(sum_f);
        } else {
            output[(long)token_id * size_n + n_out] = static_cast<T>(sum_f);
        }
    }
}

template [[host_name("wna16_moe_gemv_half_4")]] [[kernel]]
void wna16_moe_gemv_kernel<half, 4>(
    device const half*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device half*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

template [[host_name("wna16_moe_gemv_half_8")]] [[kernel]]
void wna16_moe_gemv_kernel<half, 8>(
    device const half*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device half*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

#if defined(__HAVE_BFLOAT__)
template [[host_name("wna16_moe_gemv_bfloat16_4")]] [[kernel]]
void wna16_moe_gemv_kernel<bfloat16_t, 4>(
    device const bfloat16_t*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device bfloat16_t*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

template [[host_name("wna16_moe_gemv_bfloat16_8")]] [[kernel]]
void wna16_moe_gemv_kernel<bfloat16_t, 8>(
    device const bfloat16_t*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device bfloat16_t*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);
#endif

template [[host_name("wna16_moe_gemm_half_4_32_32")]] [[kernel]]
void wna16_moe_gemm_kernel<half, 4, 32, 32>(
    device const half*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device half*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

template [[host_name("wna16_moe_gemm_half_8_32_32")]] [[kernel]]
void wna16_moe_gemm_kernel<half, 8, 32, 32>(
    device const half*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device half*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

#if defined(__HAVE_BFLOAT__)
template [[host_name("wna16_moe_gemm_bfloat16_4_32_32")]] [[kernel]]
void wna16_moe_gemm_kernel<bfloat16_t, 4, 32, 32>(
    device const bfloat16_t*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device bfloat16_t*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

template [[host_name("wna16_moe_gemm_bfloat16_8_32_32")]] [[kernel]]
void wna16_moe_gemm_kernel<bfloat16_t, 8, 32, 32>(
    device const bfloat16_t*, device const uint*, device const float*,
    device const int*, device const int*, device const float*, device bfloat16_t*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);
#endif
