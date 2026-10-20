#include "metal_dtype.metal"
#include <metal_stdlib>

using namespace metal;

// FP8 MoE GEMM kernel for Metal.
//
// Computes: output[m, n] = input[token_id, :] @ weight[expert_id, n, :]^T * scale
// where token_id = sorted_token_ids[m] / topk, expert_id = expert_ids[m]
//
// Weight format: U8 (FP8 E4M3), [num_experts, N, K]
// Scale format:  F32 block scales, [num_experts, ceil(N/block_size_n), ceil(K/block_size_k)]
// Input format:  F16/BF16, [num_input_tokens, K]
// Output format: F16/BF16, [size_m, N]

#define WARP_SIZE 32

inline float get_moe_scale(const device float* scale,
                           int expert_id, int n, int k,
                           int num_experts, int size_n, int size_k,
                           int block_size_n, int block_size_k) {
  int sn = n / block_size_n;
  int sk = k / block_size_k;
  int scale_cols = (size_k + block_size_k - 1) / block_size_k;
  int scale_rows = (size_n + block_size_n - 1) / block_size_n;
  return scale[expert_id * scale_rows * scale_cols + sn * scale_cols + sk];
}

// GEMV kernel: one simdgroup per output element (m, n).
// Grid: (N, size_m, 1), threads_per_group: 32
template <typename T>
[[kernel]] void fp8_moe_gemv_kernel(
    device const T       *input           [[ buffer(0) ]],
    device const uint8_t *weights         [[ buffer(1) ]],
    device const float   *weight_scales   [[ buffer(2) ]],
    device const int     *sorted_token_ids[[ buffer(3) ]],
    device const int     *expert_ids      [[ buffer(4) ]],
    device const float   *topk_weights    [[ buffer(5) ]],
    device       T       *output          [[ buffer(6) ]],
    constant     int     &num_experts     [[ buffer(7) ]],
    constant     int     &topk            [[ buffer(8) ]],
    constant     int     &size_m          [[ buffer(9) ]],
    constant     int     &size_n          [[ buffer(10) ]],
    constant     int     &size_k          [[ buffer(11) ]],
    constant     int     &has_topk_weights[[ buffer(12) ]],
    constant     int     &block_size_n    [[ buffer(13) ]],
    constant     int     &block_size_k    [[ buffer(14) ]],
    uint3 gid            [[ threadgroup_position_in_grid ]],
    uint  lane_id        [[ thread_index_in_simdgroup ]]
) {
    int n_out = gid.x;
    int m_out = gid.y;

    if (n_out >= size_n || m_out >= size_m) return;

    int token_id = sorted_token_ids[m_out];
    int expert_id = expert_ids[m_out];

    int input_token = token_id / topk;

    if (expert_id < 0 || expert_id >= num_experts) {
        if (lane_id == 0) {
            output[m_out * size_n + n_out] = T(0);
        }
        return;
    }

    device const T* x_ptr = input + input_token * size_k;
    device const uint8_t* w_ptr = weights + ((long)expert_id * size_n + n_out) * size_k;

    float sum_f = 0.0f;

    for (int k = lane_id * 16; k < size_k; k += WARP_SIZE * 16) {
        if (k + 15 < size_k) {
            float s = get_moe_scale(weight_scales, expert_id, n_out, k,
                                    num_experts, size_n, size_k,
                                    block_size_n, block_size_k);

            uint4 w_u4 = *(device const uint4*)(w_ptr + k);
            half4 wh_0 = scaled_vec_conversion<half4, uint32_t>(w_u4.x, s);
            half4 wh_1 = scaled_vec_conversion<half4, uint32_t>(w_u4.y, s);
            half4 wh_2 = scaled_vec_conversion<half4, uint32_t>(w_u4.z, s);
            half4 wh_3 = scaled_vec_conversion<half4, uint32_t>(w_u4.w, s);

            if constexpr (is_same_v<T, bfloat16_t>) {
                T x0 = x_ptr[k+0]; T x1 = x_ptr[k+1]; T x2 = x_ptr[k+2]; T x3 = x_ptr[k+3];
                sum_f += dot(float4(wh_0), float4(float(x0), float(x1), float(x2), float(x3)));
                T x4 = x_ptr[k+4]; T x5 = x_ptr[k+5]; T x6 = x_ptr[k+6]; T x7 = x_ptr[k+7];
                sum_f += dot(float4(wh_1), float4(float(x4), float(x5), float(x6), float(x7)));
                T x8 = x_ptr[k+8]; T x9 = x_ptr[k+9]; T x10 = x_ptr[k+10]; T x11 = x_ptr[k+11];
                sum_f += dot(float4(wh_2), float4(float(x8), float(x9), float(x10), float(x11)));
                T x12 = x_ptr[k+12]; T x13 = x_ptr[k+13]; T x14 = x_ptr[k+14]; T x15 = x_ptr[k+15];
                sum_f += dot(float4(wh_3), float4(float(x12), float(x13), float(x14), float(x15)));
            } else {
                half4 xh_0 = *(device const half4*)(x_ptr + k);
                half4 xh_1 = *(device const half4*)(x_ptr + k + 4);
                half4 xh_2 = *(device const half4*)(x_ptr + k + 8);
                half4 xh_3 = *(device const half4*)(x_ptr + k + 12);
                sum_f += dot(float4(wh_0), float4(xh_0));
                sum_f += dot(float4(wh_1), float4(xh_1));
                sum_f += dot(float4(wh_2), float4(xh_2));
                sum_f += dot(float4(wh_3), float4(xh_3));
            }
        } else {
            float s = get_moe_scale(weight_scales, expert_id, n_out, k,
                                    num_experts, size_n, size_k,
                                    block_size_n, block_size_k);
            for (int i = 0; i < 16 && k + i < size_k; ++i) {
                uint8_t w = w_ptr[k + i];
                float wf = softmax_fp8_to_float(w) * s;
                float xf;
                if constexpr (is_same_v<T, bfloat16_t>) xf = float(x_ptr[k + i]);
                else xf = float((half)x_ptr[k + i]);
                sum_f += wf * xf;
            }
        }
    }

    sum_f = simd_sum(sum_f);

    if (lane_id == 0) {
        if (has_topk_weights != 0) {
            float tw = topk_weights[m_out];
            sum_f *= tw;
        }
        if constexpr (is_same_v<T, bfloat16_t>) {
            output[m_out * size_n + n_out] = static_cast<bfloat16_t>(sum_f);
        } else {
            output[m_out * size_n + n_out] = static_cast<T>(sum_f);
        }
    }
}

// GEMM kernel: tiled, one simdgroup per (m_block, n) pair.
// Grid: ((N+31)/32, (size_m+31)/32, 1), threads_per_group: 32
template <typename T, int BLOCK_M, int BLOCK_N>
[[kernel]] void fp8_moe_gemm_kernel(
    device const T       *input           [[ buffer(0) ]],
    device const uint8_t *weights         [[ buffer(1) ]],
    device const float   *weight_scales   [[ buffer(2) ]],
    device const int     *sorted_token_ids[[ buffer(3) ]],
    device const int     *expert_ids      [[ buffer(4) ]],
    device const float   *topk_weights    [[ buffer(5) ]],
    device       T       *output          [[ buffer(6) ]],
    constant     int     &num_experts     [[ buffer(7) ]],
    constant     int     &topk            [[ buffer(8) ]],
    constant     int     &size_m          [[ buffer(9) ]],
    constant     int     &size_n          [[ buffer(10) ]],
    constant     int     &size_k          [[ buffer(11) ]],
    constant     int     &has_topk_weights[[ buffer(12) ]],
    constant     int     &block_size_n    [[ buffer(13) ]],
    constant     int     &block_size_k    [[ buffer(14) ]],
    uint3 gid            [[ threadgroup_position_in_grid ]],
    uint  lane_id        [[ thread_index_in_simdgroup ]]
) {
    int n_out = int(gid.x) * BLOCK_N + int(lane_id);
    int m_block_base = int(gid.y) * BLOCK_M;

    if (n_out >= size_n) return;

    for (int mb = 0; mb < BLOCK_M; ++mb) {
        int m_out = m_block_base + mb;
        if (m_out >= size_m) break;

        int token_id = sorted_token_ids[m_out];
        int expert_id = expert_ids[m_out];
        int input_token = token_id / topk;

        if (expert_id < 0 || expert_id >= num_experts) {
            output[m_out * size_n + n_out] = T(0);
            continue;
        }

        device const T* x_ptr = input + input_token * size_k;
        device const uint8_t* w_ptr = weights + ((long)expert_id * size_n + n_out) * size_k;

        float sum_f = 0.0f;

        for (int k = 0; k < size_k; k += 16) {
            float s = get_moe_scale(weight_scales, expert_id, n_out, k,
                                    num_experts, size_n, size_k,
                                    block_size_n, block_size_k);

            if (k + 15 < size_k) {
                uint4 w_u4 = *(device const uint4*)(w_ptr + k);
                half4 wh_0 = scaled_vec_conversion<half4, uint32_t>(w_u4.x, s);
                half4 wh_1 = scaled_vec_conversion<half4, uint32_t>(w_u4.y, s);
                half4 wh_2 = scaled_vec_conversion<half4, uint32_t>(w_u4.z, s);
                half4 wh_3 = scaled_vec_conversion<half4, uint32_t>(w_u4.w, s);

                if constexpr (is_same_v<T, bfloat16_t>) {
                    T x0 = x_ptr[k+0]; T x1 = x_ptr[k+1]; T x2 = x_ptr[k+2]; T x3 = x_ptr[k+3];
                    sum_f += dot(float4(wh_0), float4(float(x0), float(x1), float(x2), float(x3)));
                    T x4 = x_ptr[k+4]; T x5 = x_ptr[k+5]; T x6 = x_ptr[k+6]; T x7 = x_ptr[k+7];
                    sum_f += dot(float4(wh_1), float4(float(x4), float(x5), float(x6), float(x7)));
                    T x8 = x_ptr[k+8]; T x9 = x_ptr[k+9]; T x10 = x_ptr[k+10]; T x11 = x_ptr[k+11];
                    sum_f += dot(float4(wh_2), float4(float(x8), float(x9), float(x10), float(x11)));
                    T x12 = x_ptr[k+12]; T x13 = x_ptr[k+13]; T x14 = x_ptr[k+14]; T x15 = x_ptr[k+15];
                    sum_f += dot(float4(wh_3), float4(float(x12), float(x13), float(x14), float(x15)));
                } else {
                    half4 xh_0 = *(device const half4*)(x_ptr + k);
                    half4 xh_1 = *(device const half4*)(x_ptr + k + 4);
                    half4 xh_2 = *(device const half4*)(x_ptr + k + 8);
                    half4 xh_3 = *(device const half4*)(x_ptr + k + 12);
                    sum_f += dot(float4(wh_0), float4(xh_0));
                    sum_f += dot(float4(wh_1), float4(xh_1));
                    sum_f += dot(float4(wh_2), float4(xh_2));
                    sum_f += dot(float4(wh_3), float4(xh_3));
                }
            } else {
                for (int i = 0; i < 16 && k + i < size_k; ++i) {
                    uint8_t w = w_ptr[k + i];
                    float wf = softmax_fp8_to_float(w) * s;
                    float xf;
                    if constexpr (is_same_v<T, bfloat16_t>) xf = float(x_ptr[k + i]);
                    else xf = float((half)x_ptr[k + i]);
                    sum_f += wf * xf;
                }
            }
        }

        if (has_topk_weights != 0) {
            float tw = topk_weights[m_out];
            sum_f *= tw;
        }
        if constexpr (is_same_v<T, bfloat16_t>) {
            output[m_out * size_n + n_out] = static_cast<bfloat16_t>(sum_f);
        } else {
            output[m_out * size_n + n_out] = static_cast<T>(sum_f);
        }
    }
}

// Instantiations

template [[host_name("fp8_moe_gemv_half")]] [[kernel]]
void fp8_moe_gemv_kernel<half>(
    device const half*, device const uint8_t*, device const float*,
    device const int*, device const int*, device const float*, device half*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

#if defined(__HAVE_BFLOAT__)
template [[host_name("fp8_moe_gemv_bfloat16")]] [[kernel]]
void fp8_moe_gemv_kernel<bfloat16_t>(
    device const bfloat16_t*, device const uint8_t*, device const float*,
    device const int*, device const int*, device const float*, device bfloat16_t*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);
#endif

template [[host_name("fp8_moe_gemm_half_32_32")]] [[kernel]]
void fp8_moe_gemm_kernel<half, 32, 32>(
    device const half*, device const uint8_t*, device const float*,
    device const int*, device const int*, device const float*, device half*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

#if defined(__HAVE_BFLOAT__)
template [[host_name("fp8_moe_gemm_bfloat16_32_32")]] [[kernel]]
void fp8_moe_gemm_kernel<bfloat16_t, 32, 32>(
    device const bfloat16_t*, device const uint8_t*, device const float*,
    device const int*, device const int*, device const float*, device bfloat16_t*,
    constant int&, constant int&, constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);
#endif
