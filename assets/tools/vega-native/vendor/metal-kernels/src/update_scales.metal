/**
 * @brief Metal kernel used for kv scales update when using fp8 kvcache.
 * Copyright (c) 2025, Guoqing Bao.  All rights reserved.
 * This kernel computes k_scale and v_scale and update existing ones during fp8 kvcache computation
 *
 * This Metal kernel is part of the xInfer (vLLM.rs) project:
 * https://github.com/guoqingbao/attention.rs/tree/main/src/metal-kernels/src/update_kvscales.metal
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#include "metal_dtype.metal"
#include <metal_stdlib>
using namespace metal;

#define DIV_CONST 240.0f

template <typename T>
float to_float_abs(T x);

// FP32 specialization
template <>
float to_float_abs<float>(float x) {
    return fabs(x);
}

// FP16 specialization
template <>
float to_float_abs<half>(half x) {
    return fabs(float(x));  // Convert half to float for the operation
}

// BF16 specialization
template <>
float to_float_abs<bfloat16_t>(bfloat16_t x) {
    return fabs(float(x));  // Convert bfloat16 to float for the operation
}


#define DIVIDE_ROUND_UP(a, b) (((a) + (b) - 1) / (b))
#define THREADS_PER_TG 256

template <typename T>
kernel void compute_and_update_scales_per_head_kernel(
    device const T* k [[buffer(0)]],
    device const T* v [[buffer(1)]],
    constant long& num_tokens [[buffer(2)]],
    constant int& num_heads [[buffer(3)]],
    constant int& head_dim [[buffer(4)]],
    device float* k_scales [[buffer(5)]],
    device float* v_scales [[buffer(6)]],
    uint3 threadIdx [[thread_position_in_threadgroup]]
) {
    const uint tid = threadIdx.x;
    if (tid >= THREADS_PER_TG) {
        return;
    }

    threadgroup float s_k[THREADS_PER_TG];
    threadgroup float s_v[THREADS_PER_TG];

    const long total_elems = num_tokens * (long)num_heads * (long)head_dim;
    long idx = (long)tid;
    long stride = (long)THREADS_PER_TG;

    float local_max_k = 0.0f;
    float local_max_v = 0.0f;

    for (long i = idx; i < total_elems; i += stride) {
        float avk = to_float_abs<T>(k[i]);
        float avv = to_float_abs<T>(v[i]);
        if (avk > local_max_k) local_max_k = avk;
        if (avv > local_max_v) local_max_v = avv;
    }

    s_k[tid] = local_max_k;
    s_v[tid] = local_max_v;
    threadgroup_barrier(mem_flags::mem_threadgroup);

    for (int s = THREADS_PER_TG >> 1; s > 0; s >>= 1) {
        if (tid < s) {
            if (s_k[tid + s] > s_k[tid]) s_k[tid] = s_k[tid + s];
            if (s_v[tid + s] > s_v[tid]) s_v[tid] = s_v[tid + s];
        }
        threadgroup_barrier(mem_flags::mem_threadgroup);
    }

    if (tid == 0) {
        float candidate_k_scale = s_k[0] / DIV_CONST;
        float candidate_v_scale = s_v[0] / DIV_CONST;
        k_scales[0] = max(k_scales[0], candidate_k_scale);
        v_scales[0] = max(v_scales[0], candidate_v_scale);
    }
}

#define instantiate_compute_and_update_scales_per_head(type)        \
  template [[host_name("compute_and_update_scales_per_head_" #type)]]                \
  kernel void compute_and_update_scales_per_head_kernel<type>( \
    device const type* k [[buffer(0)]],                    \
    device const type* v [[buffer(1)]],                    \
    constant long& num_tokens [[buffer(2)]],               \
    constant int& num_heads [[buffer(3)]],                 \
    constant int& head_dim [[buffer(4)]],                  \
    device float* k_scales [[buffer(5)]],                  \
    device float* v_scales [[buffer(6)]],                  \
    uint3 threadIdx [[thread_position_in_threadgroup]]);

instantiate_compute_and_update_scales_per_head(float)
instantiate_compute_and_update_scales_per_head(half)
instantiate_compute_and_update_scales_per_head(bfloat16_t)
