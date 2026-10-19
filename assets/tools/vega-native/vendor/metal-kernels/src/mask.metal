#include "metal_dtype.metal"
#include <metal_stdlib>
using namespace metal;

template <typename T>
T neg_inf_value();

// FP32 specialization
template <>
float neg_inf_value<float>() {
    return -INFINITY; // Use standard -inf
}

// FP16 specialization
template <>
half neg_inf_value<half>() {
    return half(-INFINITY);
}

// BF16 specialization
template <>
bfloat16_t neg_inf_value<bfloat16_t>() {
    return bfloat16_t(-INFINITY);
}

template <typename T>
kernel void causal_mask_upper_inf(
    device T* out [[buffer(0)]],
    constant int& tgt_len [[buffer(1)]],
    constant int& sliding_window [[buffer(2)]],
    // 1D Grid: Group ID is the row index
    uint tgid [[threadgroup_position_in_grid]],
    // 1D Group: Thread ID is the local column offset
    uint tid [[thread_position_in_threadgroup]],
    // Get the size of the threadgroup (e.g., 256)
    uint threads_per_group [[threads_per_threadgroup]]
) {
    const int EPT = 4;

    // Get row index `i` from the group ID
    int i = tgid;
    
    // Safety check (should be redundant if host launches tgt_len groups)
    if (i >= tgt_len) return;

    // Calculate this thread's starting column index
    // This is the base column index for this thread (0, 4, 8, ...)
    int thread_col_base = tid * EPT;
    
    // How many columns this entire *group* can process in one pass
    // (e.g., 256 threads * 4 elements/thread = 1024)
    int cols_per_group_pass = threads_per_group * EPT;

    ulong row_offset = (ulong)i * (ulong)tgt_len;
    const T neg_inf = neg_inf_value<T>();

    // Loop over the row in blocks of `cols_per_group_pass`
    // This is necessary if tgt_len > 1024
    for (int j_block_start = 0; j_block_start < tgt_len; j_block_start += cols_per_group_pass)
    {
        // Process EPT elements for this thread
        #pragma unroll
        for (int e = 0; e < EPT; ++e) {
            int j = j_block_start + thread_col_base + e;

            if (j >= tgt_len) break;

            if (j > i || (sliding_window > 0 && j + sliding_window < i)) {
                out[row_offset + j] = neg_inf;
            }
        }
    }
}

#define instantiate_causal_mask(type)                         \
  template [[host_name("causal_mask_" #type)]]                \
  kernel void causal_mask_upper_inf<type>(                   \
      device type* out [[buffer(0)]],                         \
      constant int& tgt_len [[buffer(1)]],                    \
      constant int& sliding_window [[buffer(2)]],             \
      uint tgid [[threadgroup_position_in_grid]],             \
      uint tid [[thread_position_in_threadgroup]],             \
      uint threads_per_group [[threads_per_threadgroup]]);

instantiate_causal_mask(float)
instantiate_causal_mask(bfloat16_t)
instantiate_causal_mask(half)