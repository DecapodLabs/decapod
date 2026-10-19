#include "metal_dtype.metal"
#include <metal_stdlib>
#include <metal_simdgroup_matrix>

using namespace metal;

// -----------------------------------------------------------------------------
// Constants & Helper Types
// -----------------------------------------------------------------------------
#define SIMD_SIZE 32
#define WARP_SIZE 32

// Helper to convert FP8 (represented as uint8_t) to float, then cast to half
inline half fp8_to_half(uint8_t val, float scale) {
    return static_cast<half>(softmax_fp8_to_float(val) * scale);
}

// Helper to get scale with stride handling
inline float get_scale(const device float* scale,
                       int n, int k, int scale_stride,
                       int block_size_y, int block_size_x) {
  int sr = n / block_size_y;
  int sc = k / block_size_x;
  return scale[sr * scale_stride + sc];
}

// -----------------------------------------------------------------------------
// AMX-based Blocked Kernel (For M > 1)
// -----------------------------------------------------------------------------
template <typename T, int BLOCK_M, int BLOCK_N, int BLOCK_K>
[[kernel]] void fp8_matmul_kernel(
    device const T  *input        [[ buffer(0) ]],
    device const uint8_t *weight       [[ buffer(1) ]],
    device const float *weight_scale [[ buffer(2) ]],
    device       T  *output       [[ buffer(3) ]],
    constant     int   &M            [[ buffer(4) ]],
    constant     int   &N            [[ buffer(5) ]],
    constant     int   &K            [[ buffer(6) ]],
    constant     int   &row_stride   [[ buffer(7) ]],
    constant     int   &block_size_y [[ buffer(8) ]],
    constant     int   &block_size_x [[ buffer(9) ]],
    uint2 gid [[ threadgroup_position_in_grid ]],
    uint  simd_lane_id [[ thread_index_in_simdgroup ]]
) {
    // -------------------------------------------------------------------------
    // 1. Threadgroup Memory (Shared Memory)
    // -------------------------------------------------------------------------
    threadgroup half s_a[BLOCK_M][BLOCK_K];
    threadgroup half s_b[BLOCK_K][BLOCK_N];

    // Accumulators
    simdgroup_matrix<float, 8, 8> acc[BLOCK_M/8][BLOCK_N/8];
    
    // Initialize accumulators to 0
    #pragma unroll
    for (int i = 0; i < BLOCK_M/8; ++i) {
        #pragma unroll
        for (int j = 0; j < BLOCK_N/8; ++j) {
            acc[i][j] = make_filled_simdgroup_matrix<float, 8, 8>(0.0f);
        }
    }

    // Global Offsets
    int global_row_base = gid.y * BLOCK_M;
    int global_col_base = gid.x * BLOCK_N;

    // Linear thread index within the group (0-31)
    int tid = simd_lane_id; 

    // -------------------------------------------------------------------------
    // 2. Main Loop over K
    // -------------------------------------------------------------------------
    for (int k = 0; k < K; k += BLOCK_K) {
        
        // --- Load A (T) -> Dequant/Cast -> Threadgroup (half) ---
        for (int i = 0; i < BLOCK_M; ++i) {
            int r = i; 
            int c = tid; 
            
            int gr = global_row_base + r;
            int gc = k + c;
            
            half val = 0.0h;
            if (gr < M && gc < K) {
                // Load T and convert to half
                if constexpr (is_same_v<T, bfloat16_t>) {
                     val = static_cast<half>(static_cast<float>(input[gr * K + gc]));
                } else {
                     val = static_cast<half>(input[gr * K + gc]);
                }
            }
            s_a[r][c] = val;
        }

        threadgroup_barrier(mem_flags::mem_threadgroup);
        
        // --- Load B (TRANSPOSED in Shared Mem) ---
        // Optimization: Hoist scale loading.
        float tile_scale = 1.0f;
        if (global_col_base < N && k < K) {
             tile_scale = get_scale(weight_scale, global_col_base, k, row_stride, block_size_y, block_size_x);
        }
        
        // Vectorized Load Strategy (Row-wise from Global, Col-wise to Shared)
        int n_local = tid; // Row of the tile we are loading (0..31)
        int gn = global_col_base + n_local;
        
        if (gn < N) {
             // We load 32 bytes from `weight[gn][k..k+31]`.
             const device uint* w_row = (const device uint*)(weight + gn * K + k);
             
             #pragma unroll
             for (int j = 0; j < 8; ++j) {
                  uint val_u = w_row[j];
                  half4 h4 = scaled_vec_conversion<half4, uint32_t>(val_u, tile_scale);
                  
                  int k_idx_base = j * 4;
                  s_b[k_idx_base + 0][n_local] = h4.x;
                  s_b[k_idx_base + 1][n_local] = h4.y;
                  s_b[k_idx_base + 2][n_local] = h4.z;
                  s_b[k_idx_base + 3][n_local] = h4.w;
             }
        } else {
             #pragma unroll
             for (int j = 0; j < 32; ++j) {
                  s_b[j][n_local] = 0.0h;
             }
        }
        
        threadgroup_barrier(mem_flags::mem_threadgroup);

        // --- Compute (SIMD Matrix MMA) ---
        for (int k_step = 0; k_step < BLOCK_K; k_step += 8) {
            #pragma unroll
            for (int row_tile = 0; row_tile < BLOCK_M/8; ++row_tile) {
                simdgroup_matrix<half, 8, 8> fragA;
                simdgroup_load(fragA, &s_a[row_tile * 8][k_step], BLOCK_K,
                               ulong2(0,0), false);
                #pragma unroll
                for (int col_tile = 0; col_tile < BLOCK_N/8; ++col_tile) {
                    simdgroup_matrix<half, 8, 8> fragB;
                    simdgroup_load(fragB, &s_b[k_step][col_tile * 8 + 0], BLOCK_N, ulong2(0,0), false);
                    simdgroup_multiply_accumulate(acc[row_tile][col_tile], fragA, fragB, acc[row_tile][col_tile]);
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    // -------------------------------------------------------------------------
    // 3. Store Results
    // -------------------------------------------------------------------------
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
         
    if (local_r < BLOCK_M && global_r < M) { 
         for (int local_c = 0; local_c < BLOCK_N; ++local_c) {
              int global_c = global_col_base + local_c;
              if (global_c < N) {
                  float val = s_out[local_r][local_c];
                  if constexpr (is_same_v<T, bfloat16_t>) {
                       output[global_r * N + global_c] = static_cast<bfloat16_t>(val);
                  } else {
                       output[global_r * N + global_c] = static_cast<T>(val);
                  }
              }
         }
    }
}


// -----------------------------------------------------------------------------
// Warp-Parallel GEMV Kernel (For M = 1 / Small M)
// -----------------------------------------------------------------------------
// Specialized for Matrix [M, K] x Weight [N, K] -> Output [M, N].
// Eight SIMD groups share an activation tile and compute eight output rows.
template <typename T>
[[kernel]] void fp8_gemv_kernel(
    device const T  *input        [[ buffer(0) ]],
    device const uint8_t *weight       [[ buffer(1) ]],
    device const float *weight_scale [[ buffer(2) ]],
    device       T  *output       [[ buffer(3) ]],
    constant     int   &M            [[ buffer(4) ]],
    constant     int   &N            [[ buffer(5) ]],
    constant     int   &K            [[ buffer(6) ]],
    constant     int   &row_stride   [[ buffer(7) ]],
    constant     int   &block_size_y [[ buffer(8) ]], // N block size for scale
    constant     int   &block_size_x [[ buffer(9) ]],  // K block size for scale
    uint3 gid [[threadgroup_position_in_grid]],
    uint tid [[thread_index_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lane_id [[thread_index_in_simdgroup]]
) {
    constexpr int OUTPUTS_PER_TG = 8;
    constexpr int K_TILE = 1024;
    threadgroup half x_tile[K_TILE];
    int n_out = int(gid.x) * OUTPUTS_PER_TG + int(simd_gid);
    int m_out = gid.y;
    if (m_out >= M) return;
    device const T* x_ptr = input + m_out * K;
    float sum_f = 0.0f;

    for (int kb = 0; kb < K; kb += K_TILE) {
        for (int i = int(tid) * 4; i < K_TILE; i += 1024) {
#pragma unroll
            for (int j = 0; j < 4; ++j) {
                int kk = kb + i + j;
                x_tile[i + j] = kk < K ? half(float(x_ptr[kk])) : half(0.0h);
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
        if (n_out < N) {
            device const uint8_t* w_ptr = weight + n_out * K;
            int k = kb + int(simd_lane_id) * 32;
            if (k + 31 < K) {
#pragma unroll
                for (int chunk = 0; chunk < 2; ++chunk) {
                    int kc = k + chunk * 16;
                    float s = get_scale(weight_scale, n_out, kc, row_stride,
                                        block_size_y, block_size_x);
                    uint4 p = *(device const uint4*)(w_ptr + kc);
                    half4 w0 = scaled_vec_conversion<half4, uint32_t>(p.x, s);
                    half4 w1 = scaled_vec_conversion<half4, uint32_t>(p.y, s);
                    half4 w2 = scaled_vec_conversion<half4, uint32_t>(p.z, s);
                    half4 w3 = scaled_vec_conversion<half4, uint32_t>(p.w, s);
                    int xi = int(simd_lane_id) * 32 + chunk * 16;
                    sum_f += dot(float4(w0), float4(*(threadgroup half4*)(x_tile + xi)));
                    sum_f += dot(float4(w1), float4(*(threadgroup half4*)(x_tile + xi + 4)));
                    sum_f += dot(float4(w2), float4(*(threadgroup half4*)(x_tile + xi + 8)));
                    sum_f += dot(float4(w3), float4(*(threadgroup half4*)(x_tile + xi + 12)));
                }
            } else {
                for (int kk = k; kk < min(k + 32, K); ++kk) {
                    float s = get_scale(weight_scale, n_out, kk, row_stride,
                                        block_size_y, block_size_x);
                    sum_f = fma(float(x_tile[kk - kb]),
                                softmax_fp8_to_float(w_ptr[kk]) * s, sum_f);
                }
            }
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }
    sum_f = simd_sum(sum_f);
    if (n_out < N && simd_lane_id == 0)
        output[m_out * N + n_out] = static_cast<T>(sum_f);
}


// -----------------------------------------------------------------------------
// Instantiations
// -----------------------------------------------------------------------------

// Standard AMX Kernels
template [[host_name("fp8_matmul_half_32_32_32")]] [[kernel]] void fp8_matmul_kernel<half, 32, 32, 32>(
    device const half* input [[buffer(0)]],
    device const uint8_t* weight [[buffer(1)]],
    device const float* weight_scale [[buffer(2)]],
    device half* output [[buffer(3)]],
    constant int& M [[buffer(4)]],
    constant int& N [[buffer(5)]],
    constant int& K [[buffer(6)]],
    constant int& row_stride [[buffer(7)]],
    constant int& block_size_y [[buffer(8)]],
    constant int& block_size_x [[buffer(9)]],
    uint2 gid [[ threadgroup_position_in_grid ]],
    uint simd_lane_id [[ thread_index_in_simdgroup ]]
);

#if defined(__HAVE_BFLOAT__)
template [[host_name("fp8_matmul_bfloat16_32_32_32")]] [[kernel]] void fp8_matmul_kernel<bfloat16_t, 32, 32, 32>(
    device const bfloat16_t* input [[buffer(0)]],
    device const uint8_t* weight [[buffer(1)]],
    device const float* weight_scale [[buffer(2)]],
    device bfloat16_t* output [[buffer(3)]],
    constant int& M [[buffer(4)]],
    constant int& N [[buffer(5)]],
    constant int& K [[buffer(6)]],
    constant int& row_stride [[buffer(7)]],
    constant int& block_size_y [[buffer(8)]],
    constant int& block_size_x [[buffer(9)]],
    uint2 gid [[ threadgroup_position_in_grid ]],
    uint simd_lane_id [[ thread_index_in_simdgroup ]]
);
#endif

// Small-AMX Kernels 
template [[host_name("fp8_matmul_half_16_32_32")]] [[kernel]] void fp8_matmul_kernel<half, 16, 32, 32>(
    device const half* input [[buffer(0)]],
    device const uint8_t* weight [[buffer(1)]],
    device const float* weight_scale [[buffer(2)]],
    device half* output [[buffer(3)]],
    constant int& M [[buffer(4)]],
    constant int& N [[buffer(5)]],
    constant int& K [[buffer(6)]],
    constant int& row_stride [[buffer(7)]],
    constant int& block_size_y [[buffer(8)]],
    constant int& block_size_x [[buffer(9)]],
    uint2 gid [[ threadgroup_position_in_grid ]],
    uint simd_lane_id [[ thread_index_in_simdgroup ]]
);

#if defined(__HAVE_BFLOAT__)
template [[host_name("fp8_matmul_bfloat16_16_32_32")]] [[kernel]] void fp8_matmul_kernel<bfloat16_t, 16, 32, 32>(
    device const bfloat16_t* input [[buffer(0)]],
    device const uint8_t* weight [[buffer(1)]],
    device const float* weight_scale [[buffer(2)]],
    device bfloat16_t* output [[buffer(3)]],
    constant int& M [[buffer(4)]],
    constant int& N [[buffer(5)]],
    constant int& K [[buffer(6)]],
    constant int& row_stride [[buffer(7)]],
    constant int& block_size_y [[buffer(8)]],
    constant int& block_size_x [[buffer(9)]],
    uint2 gid [[ threadgroup_position_in_grid ]],
    uint simd_lane_id [[ thread_index_in_simdgroup ]]
);
#endif

// GEMV Kernels (M=1)
template [[host_name("fp8_gemv_half")]] [[kernel]] void fp8_gemv_kernel<half>(
    device const half* input [[buffer(0)]],
    device const uint8_t* weight [[buffer(1)]],
    device const float* weight_scale [[buffer(2)]],
    device half* output [[buffer(3)]],
    constant int& M [[buffer(4)]],
    constant int& N [[buffer(5)]],
    constant int& K [[buffer(6)]],
    constant int& row_stride [[buffer(7)]],
    constant int& block_size_y [[buffer(8)]],
    constant int& block_size_x [[buffer(9)]],
    uint3 gid [[threadgroup_position_in_grid]], uint tid [[thread_index_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lane_id [[thread_index_in_simdgroup]]
);

#if defined(__HAVE_BFLOAT__)
template [[host_name("fp8_gemv_bfloat16")]] [[kernel]] void fp8_gemv_kernel<bfloat16_t>(
    device const bfloat16_t* input [[buffer(0)]],
    device const uint8_t* weight [[buffer(1)]],
    device const float* weight_scale [[buffer(2)]],
    device bfloat16_t* output [[buffer(3)]],
    constant int& M [[buffer(4)]],
    constant int& N [[buffer(5)]],
    constant int& K [[buffer(6)]],
    constant int& row_stride [[buffer(7)]],
    constant int& block_size_y [[buffer(8)]],
    constant int& block_size_x [[buffer(9)]],
    uint3 gid [[threadgroup_position_in_grid]], uint tid [[thread_index_in_threadgroup]],
    uint simd_gid [[simdgroup_index_in_threadgroup]],
    uint simd_lane_id [[thread_index_in_simdgroup]]
);
#endif
