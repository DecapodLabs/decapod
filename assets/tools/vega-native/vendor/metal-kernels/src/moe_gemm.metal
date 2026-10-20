#include "metal_dtype.metal"
#include <metal_stdlib>

using namespace metal;

// MoE GEMM kernel for Metal
//
// Computes: output[m, n] = input[token_id, :] @ weight[expert_id, n, :]^T
// where token_id = sorted_token_ids[m] / topk, expert_id = expert_ids[m]
//
// Optionally applies topk_weights for weighted accumulation in the down
// projection pass.
//
// Weight layout: [num_experts, N, K] (row-major per expert, N output rows each of K elements)
// Input layout:  [num_input_tokens, K]
// Output layout: [size_m, N]

#define WARP_SIZE 32

// GEMV kernel: one simdgroup per output element (m, n).
// Grid: (N, size_m, 1), threads_per_group: 32
template <typename T>
[[kernel]] void moe_gemv_kernel(
    device const T       *input           [[ buffer(0) ]],   // [num_input_tokens, K]
    device const T       *weights         [[ buffer(1) ]],   // [num_experts, N, K]
    device const int     *sorted_token_ids[[ buffer(2) ]],
    device const int     *expert_ids      [[ buffer(3) ]],
    device const float   *topk_weights    [[ buffer(4) ]],   // nullable
    device       T       *output          [[ buffer(5) ]],   // [size_m, N]
    constant     int     &num_experts     [[ buffer(6) ]],
    constant     int     &topk            [[ buffer(7) ]],
    constant     int     &size_m          [[ buffer(8) ]],
    constant     int     &size_n          [[ buffer(9) ]],
    constant     int     &size_k          [[ buffer(10) ]],
    constant     int     &has_topk_weights[[ buffer(11) ]],
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
    device const T* w_ptr = weights + ((long)expert_id * size_n + n_out) * size_k;

    float sum_f = 0.0f;

    // Each lane processes elements stride WARP_SIZE apart, 4 at a time
    for (int k = lane_id * 4; k < size_k; k += WARP_SIZE * 4) {
        if (k + 3 < size_k) {
            float4 xv = float4(
                float(x_ptr[k]),   float(x_ptr[k+1]),
                float(x_ptr[k+2]), float(x_ptr[k+3])
            );
            float4 wv = float4(
                float(w_ptr[k]),   float(w_ptr[k+1]),
                float(w_ptr[k+2]), float(w_ptr[k+3])
            );
            sum_f += dot(xv, wv);
        } else {
            for (int i = 0; i < 4 && k + i < size_k; ++i) {
                sum_f += float(x_ptr[k+i]) * float(w_ptr[k+i]);
            }
        }
    }

    sum_f = simd_sum(sum_f);

    if (lane_id == 0) {
        if (has_topk_weights != 0) {
            float tw = topk_weights[token_id];
            output[m_out * size_n + n_out] = T(sum_f * tw);
        } else {
            output[m_out * size_n + n_out] = T(sum_f);
        }
    }
}

// GEMM kernel using simdgroup_matrix for larger M.
// Uses tiled approach: BLOCK_M x BLOCK_N output tile per threadgroup.
// Grid: (ceil(N/BLOCK_N), ceil(size_m/BLOCK_M), 1), threads_per_group: 32
template <typename T, int BLOCK_M, int BLOCK_N, int BLOCK_K>
[[kernel]] void moe_gemm_kernel(
    device const T       *input           [[ buffer(0) ]],
    device const T       *weights         [[ buffer(1) ]],
    device const int     *sorted_token_ids[[ buffer(2) ]],
    device const int     *expert_ids      [[ buffer(3) ]],
    device const float   *topk_weights    [[ buffer(4) ]],
    device       T       *output          [[ buffer(5) ]],
    constant     int     &num_experts     [[ buffer(6) ]],
    constant     int     &topk            [[ buffer(7) ]],
    constant     int     &size_m          [[ buffer(8) ]],
    constant     int     &size_n          [[ buffer(9) ]],
    constant     int     &size_k          [[ buffer(10) ]],
    constant     int     &has_topk_weights[[ buffer(11) ]],
    uint3 gid            [[ threadgroup_position_in_grid ]],
    uint  lane_id        [[ thread_index_in_simdgroup ]]
) {
    threadgroup half s_a[BLOCK_M][BLOCK_K];
    threadgroup half s_b[BLOCK_K][BLOCK_N];

    simdgroup_matrix<float, 8, 8> acc[BLOCK_M/8][BLOCK_N/8];

    #pragma unroll
    for (int i = 0; i < BLOCK_M/8; ++i) {
        #pragma unroll
        for (int j = 0; j < BLOCK_N/8; ++j) {
            acc[i][j] = make_filled_simdgroup_matrix<float, 8, 8>(0.0f);
        }
    }

    const int target_expert = int(gid.z);
    // expert_ids is sorted. Find this expert's half-open route range so every
    // matrix tile has one B matrix and remains valid across expert boundaries.
    int lo = 0, hi = size_m;
    while (lo < hi) {
        int mid = (lo + hi) >> 1;
        if (expert_ids[mid] < target_expert) lo = mid + 1; else hi = mid;
    }
    const int expert_begin = lo;
    hi = size_m;
    while (lo < hi) {
        int mid = (lo + hi) >> 1;
        if (expert_ids[mid] <= target_expert) lo = mid + 1; else hi = mid;
    }
    const int expert_end = lo;
    int global_col_base = gid.x * BLOCK_N;
    int global_row_base = expert_begin + int(gid.y) * BLOCK_M;
    if (global_row_base >= expert_end) return;

    int tid = lane_id;

    // Precompute token and expert mappings for this tile's rows
    int token_ids_local[BLOCK_M];
    int expert_ids_local[BLOCK_M];
    for (int i = 0; i < BLOCK_M; ++i) {
        int m = global_row_base + i;
        if (m < expert_end) {
            token_ids_local[i] = sorted_token_ids[m] / topk;
            expert_ids_local[i] = expert_ids[m];
        } else {
            token_ids_local[i] = 0;
            expert_ids_local[i] = -1;
        }
    }

    for (int k = 0; k < size_k; k += BLOCK_K) {
        // Load A tile: input rows selected by sorted_token_ids
        for (int i = 0; i < BLOCK_M; ++i) {
            int gc = k + tid;
            half val = 0.0h;
            if (expert_ids_local[i] >= 0 && gc < size_k) {
                val = static_cast<half>(float(input[token_ids_local[i] * size_k + gc]));
            }
            s_a[i][tid] = val;
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);

        int n_local = tid;
        int gn = global_col_base + n_local;
        if (target_expert < num_experts && gn < size_n) {
            device const T* w_row = weights + ((long)target_expert * size_n + gn) * size_k + k;
            #pragma unroll
            for (int j = 0; j < BLOCK_K; ++j) {
                s_b[j][n_local] = (k + j < size_k) ? static_cast<half>(float(w_row[j])) : 0.0h;
            }
        } else {
            #pragma unroll
            for (int j = 0; j < BLOCK_K; ++j) {
                s_b[j][n_local] = 0.0h;
            }
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);

        // Compute via simdgroup MMA
        for (int k_step = 0; k_step < BLOCK_K; k_step += 8) {
            simdgroup_matrix<half, 8, 8> fragA;
            simdgroup_matrix<half, 8, 8> fragB;

            #pragma unroll
            for (int row_tile = 0; row_tile < BLOCK_M/8; ++row_tile) {
                #pragma unroll
                for (int col_tile = 0; col_tile < BLOCK_N/8; ++col_tile) {
                    simdgroup_load(fragA, &s_a[row_tile * 8][k_step], BLOCK_K, ulong2(0,0), false);
                    simdgroup_load(fragB, &s_b[k_step][col_tile * 8], BLOCK_N, ulong2(0,0), false);
                    simdgroup_multiply_accumulate(acc[row_tile][col_tile], fragA, fragB, acc[row_tile][col_tile]);
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    // Store results
    threadgroup float s_out[BLOCK_M][BLOCK_N];

    #pragma unroll
    for (int row_tile = 0; row_tile < BLOCK_M/8; ++row_tile) {
        #pragma unroll
        for (int col_tile = 0; col_tile < BLOCK_N/8; ++col_tile) {
            simdgroup_store(acc[row_tile][col_tile], &s_out[row_tile * 8][col_tile * 8], BLOCK_N, ulong2(0,0), false);
        }
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);

    int local_r = tid;
    int global_r = global_row_base + local_r;

    if (local_r < BLOCK_M && global_r < expert_end) {
        int token_id = sorted_token_ids[global_r];
        int eid = expert_ids[global_r];
        if (eid >= 0 && eid < num_experts) {
            float tw = 1.0f;
            if (has_topk_weights != 0) {
                tw = topk_weights[token_id];
            }
            for (int local_c = 0; local_c < BLOCK_N; ++local_c) {
                int global_c = global_col_base + local_c;
                if (global_c < size_n) {
                    float val = s_out[local_r][local_c] * tw;
                    output[global_r * size_n + global_c] = T(val);
                }
            }
        }
    }
}

// Instantiations: GEMV
template [[host_name("moe_gemv_half")]] [[kernel]]
void moe_gemv_kernel<half>(
    device const half*, device const half*,
    device const int*, device const int*, device const float*,
    device half*,
    constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

#if defined(__HAVE_BFLOAT__)
template [[host_name("moe_gemv_bfloat16")]] [[kernel]]
void moe_gemv_kernel<bfloat16_t>(
    device const bfloat16_t*, device const bfloat16_t*,
    device const int*, device const int*, device const float*,
    device bfloat16_t*,
    constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);
#endif

// Instantiations: GEMM 32x32x32
template [[host_name("moe_gemm_half_32_32_32")]] [[kernel]]
void moe_gemm_kernel<half, 32, 32, 32>(
    device const half*, device const half*,
    device const int*, device const int*, device const float*,
    device half*,
    constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);

#if defined(__HAVE_BFLOAT__)
template [[host_name("moe_gemm_bfloat16_32_32_32")]] [[kernel]]
void moe_gemm_kernel<bfloat16_t, 32, 32, 32>(
    device const bfloat16_t*, device const bfloat16_t*,
    device const int*, device const int*, device const float*,
    device bfloat16_t*,
    constant int&, constant int&, constant int&,
    constant int&, constant int&, constant int&,
    uint3, uint);
#endif
