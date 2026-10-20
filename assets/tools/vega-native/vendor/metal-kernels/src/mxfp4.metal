#include "metal_dtype.metal"
#include <metal_stdlib>

using namespace metal;

// MXFP4 (Microscaling FP4 E2M1) kernels for linear and MoE GEMM.
//
// Weight format: packed U8, 2 FP4 nibbles per byte (low nibble = even k, high = k+1)
// Scale format:  U8 E8M0 exponents, one per 32 weights (MX block size)
// Weight layout: [N, K/2] for linear, [num_experts, N, K/2] for MoE
// Scale layout:  [N, K/32] for linear, [num_experts, N, K/32] for MoE

namespace mxfp4 {

constexpr constant int kWarpSize = 32;
constexpr constant int kElemsPerLane = 32;
constexpr constant int kBlockN = 8;
constexpr constant int kThreadsPerThreadgroup = kBlockN * kWarpSize;

constexpr constant int kKTile = kWarpSize * kElemsPerLane;
constexpr constant int kKTilePadded = kKTile + (kKTile / kWarpSize);

constant char kMagLut[8] = {0, 1, 2, 3, 4, 6, 8, 12};

METAL_FUNC float e8m0_to_float(uchar e) {
  return as_type<float>(uint(e) << 23);
}

METAL_FUNC char fp4_to_i8x2(uchar nibble) {
  char v = kMagLut[nibble & 7];
  return (nibble & 8) ? char(-v) : v;
}

METAL_FUNC float simdgroup_reduce_sum(float v) {
  v += simd_shuffle_xor(v, ushort(16));
  v += simd_shuffle_xor(v, ushort(8));
  v += simd_shuffle_xor(v, ushort(4));
  v += simd_shuffle_xor(v, ushort(2));
  v += simd_shuffle_xor(v, ushort(1));
  return v;
}

template <typename T>
METAL_FUNC void mxfp4_dot_k1024_tiles(const device uchar *w_row,
                                      const device uchar *s_row,
                                      const threadgroup float *x_tile_padded,
                                      thread float &acc0, thread float &acc1,
                                      int K, int k_base, ushort lane_id) {
  const int k_lane = k_base + int(lane_id) * kElemsPerLane;
  if (k_lane >= K) {
    return;
  }

  const int scale_idx = k_lane / 32;
  const float w_scale = e8m0_to_float(s_row[scale_idx]) * 0.5f;

  const device uint4 *w_ptr =
      reinterpret_cast<const device uint4 *>(w_row + (k_lane / 2));
  const uint4 packed = *w_ptr;

  const threadgroup float *in =
      x_tile_padded + int(lane_id) * (kElemsPerLane + 1);

  int in_idx = 0;

#pragma unroll
  for (int u = 0; u < 4; ++u) {
    uint vv = packed[u];
#pragma unroll
    for (int j = 0; j < 4; ++j) {
      const uchar b = uchar(vv & 0xff);
      vv >>= 8;

      const float w0 = float(fp4_to_i8x2(b & 0x0f)) * w_scale;
      const float w1 = float(fp4_to_i8x2((b >> 4) & 0x0f)) * w_scale;

      acc0 = fma(in[in_idx + 0], w0, acc0);
      acc1 = fma(in[in_idx + 1], w1, acc1);
      in_idx += 2;
    }
  }
}

// ---- Linear matmul: output[m,n] = input[m,:] @ weight[n,:]^T ----

template <typename T>
METAL_FUNC void
mxfp4_matmul_impl(const device T *x, const device uchar *w,
                  const device uchar *scales, const device T *bias, device T *y,
                  int M, int N, int K, int has_bias, threadgroup float *x_tile,
                  uint tid, ushort simd_gid, ushort lane_id, uint3 gid) {
  (void)tid;

  const int row = int(gid.y);
  const int n_base = int(gid.x) * kBlockN;
  const int n = n_base + int(simd_gid);

  if (row >= M || n >= N) {
    return;
  }

  const device T *x_row = x + row * K;
  const device uchar *w_row = w + n * (K / 2);
  const device uchar *s_row = scales + n * (K / 32);

  float acc0 = 0.0f;
  float acc1 = 0.0f;

  for (int k_base = 0; k_base < K; k_base += kKTile) {
    const int local_base = int(tid) * 4;
#pragma unroll
    for (int i = 0; i < 4; ++i) {
      const int k_local = local_base + i;
      const int k = k_base + k_local;
      const float v = (k < K) ? float(x_row[k]) : 0.0f;
      x_tile[k_local + (k_local / kWarpSize)] = v;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);

    mxfp4_dot_k1024_tiles<T>(w_row, s_row, x_tile, acc0, acc1, K, k_base,
                             lane_id);

    threadgroup_barrier(mem_flags::mem_threadgroup);
  }

  float acc = acc0 + acc1;
  acc = simdgroup_reduce_sum(acc);

  if (lane_id == 0) {
    if (has_bias != 0) {
      acc += float(bias[n]);
    }
    y[row * N + n] = T(acc);
  }
}

// ---- MoE GEMM split: one threadgroup per (token, expert_slot, n_block) ----

template <typename T>
METAL_FUNC void mxfp4_moe_gemm_split_impl(
    const device T *x, const device uchar *w, const device uchar *scales,
    const device T *biases, const device uint *indices,
    const device float *topk_weights, device T *y,
    int num_tokens, int topk, int num_experts, int N, int K, int has_bias,
    int input_has_topk_dim, int has_topk_weights,
    threadgroup float *x_tile, uint tid,
    ushort simd_gid, ushort lane_id, uint3 gid) {
  (void)tid;

  const int n_base = int(gid.x) * kBlockN;
  const int token_idx = int(gid.y);
  const int expert_slot = int(gid.z);
  const int n = n_base + int(simd_gid);

  if (token_idx >= num_tokens || expert_slot >= topk || n >= N) {
    return;
  }

  const uint expert_idx = indices[token_idx * topk + expert_slot];
  if (expert_idx >= uint(num_experts)) {
    if (lane_id == 0) {
      y[(token_idx * topk + expert_slot) * N + n] = T(0.0f);
    }
    return;
  }

  const device T *x_row = input_has_topk_dim != 0
                              ? (x + (token_idx * topk + expert_slot) * K)
                              : (x + token_idx * K);

  const int weight_row_stride = K / 2;
  const int scale_stride = K / 32;

  const device uchar *w_row =
      w + (size_t(expert_idx) * N + size_t(n)) * weight_row_stride;
  const device uchar *s_row =
      scales + (size_t(expert_idx) * N + size_t(n)) * scale_stride;

  float acc0 = 0.0f;
  float acc1 = 0.0f;

  for (int k_base = 0; k_base < K; k_base += kKTile) {
    const int local_base = int(tid) * 4;
#pragma unroll
    for (int i = 0; i < 4; ++i) {
      const int k_local = local_base + i;
      const int k = k_base + k_local;
      const float v = (k < K) ? float(x_row[k]) : 0.0f;
      x_tile[k_local + (k_local / kWarpSize)] = v;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);

    mxfp4_dot_k1024_tiles<T>(w_row, s_row, x_tile, acc0, acc1, K, k_base,
                             lane_id);

    threadgroup_barrier(mem_flags::mem_threadgroup);
  }

  float acc = acc0 + acc1;
  acc = simdgroup_reduce_sum(acc);

  if (lane_id == 0) {
    if (has_bias != 0) {
      acc += float(biases[size_t(expert_idx) * N + size_t(n)]);
    }
    if (has_topk_weights != 0) {
      acc *= topk_weights[token_idx * topk + expert_slot];
    }
    y[(token_idx * topk + expert_slot) * N + n] = T(acc);
  }
}

// ---- MoE GEMM reuse: reuse input across expert slots when topk <= 8 ----

template <typename T, int MAX_TOPK>
METAL_FUNC void mxfp4_moe_gemm_reuse_impl(
    const device T *x, const device uchar *w, const device uchar *scales,
    const device T *biases, const device uint *indices,
    const device float *topk_weights, device T *y,
    int num_tokens, int topk, int num_experts, int N, int K, int has_bias,
    int has_topk_weights,
    uint tid, ushort simd_gid, threadgroup float *x_tile, ushort lane_id,
    uint3 gid) {
  (void)tid;

  const int n_base = int(gid.x) * kBlockN;
  const int token_idx = int(gid.y);
  const int n = n_base + int(simd_gid);

  if (token_idx >= num_tokens || n >= N) {
    return;
  }
  if (topk > MAX_TOPK) {
    return;
  }

  thread uint expert_idx[MAX_TOPK];
#pragma unroll
  for (int s = 0; s < MAX_TOPK; ++s) {
    expert_idx[s] = (s < topk) ? indices[token_idx * topk + s] : 0u;
  }

  const device T *x_row = x + token_idx * K;

  const int weight_row_stride = K / 2;
  const int scale_stride = K / 32;

  float acc0[MAX_TOPK];
  float acc1[MAX_TOPK];
#pragma unroll
  for (int s = 0; s < MAX_TOPK; ++s) {
    acc0[s] = 0.0f;
    acc1[s] = 0.0f;
  }

  for (int k_base = 0; k_base < K; k_base += kKTile) {
    const int local_base = int(tid) * 4;
#pragma unroll
    for (int i = 0; i < 4; ++i) {
      const int k_local = local_base + i;
      const int k = k_base + k_local;
      const float v = (k < K) ? float(x_row[k]) : 0.0f;
      x_tile[k_local + (k_local / kWarpSize)] = v;
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);

#pragma unroll
    for (int s = 0; s < MAX_TOPK; ++s) {
      if (s >= topk) {
        continue;
      }
      const uint e = expert_idx[s];
      if (e >= uint(num_experts)) {
        continue;
      }
      const device uchar *w_row =
          w + (size_t(e) * N + size_t(n)) * weight_row_stride;
      const device uchar *s_row =
          scales + (size_t(e) * N + size_t(n)) * scale_stride;
      mxfp4_dot_k1024_tiles<T>(w_row, s_row, x_tile, acc0[s], acc1[s], K,
                               k_base, lane_id);
    }

    threadgroup_barrier(mem_flags::mem_threadgroup);
  }

#pragma unroll
  for (int s = 0; s < MAX_TOPK; ++s) {
    if (s >= topk) {
      continue;
    }
    float acc = acc0[s] + acc1[s];
    acc = simdgroup_reduce_sum(acc);
    if (lane_id == 0) {
      const uint e = expert_idx[s];
      if (e >= uint(num_experts)) {
        y[(token_idx * topk + s) * N + n] = T(0.0f);
        continue;
      }
      if (has_bias != 0) {
        acc += float(biases[size_t(e) * N + size_t(n)]);
      }
      if (has_topk_weights != 0) {
        acc *= topk_weights[token_idx * topk + s];
      }
      y[(token_idx * topk + s) * N + n] = T(acc);
    }
  }
}

} // namespace mxfp4

// ---- Kernel instantiations ----

[[kernel]] void mxfp4_matmul_f16(
    const device half *x [[buffer(0)]], const device uchar *w [[buffer(1)]],
    const device uchar *scales [[buffer(2)]],
    const device half *bias [[buffer(3)]], device half *y [[buffer(4)]],
    const constant int &M [[buffer(5)]], const constant int &N [[buffer(6)]],
    const constant int &K [[buffer(7)]],
    const constant int &has_bias [[buffer(8)]],
    uint tid [[thread_index_in_threadgroup]],
    ushort simd_gid [[simdgroup_index_in_threadgroup]],
    ushort lane_id [[thread_index_in_simdgroup]],
    uint3 gid [[threadgroup_position_in_grid]]) {
  threadgroup float x_tile[mxfp4::kKTilePadded];
  mxfp4::mxfp4_matmul_impl<half>(x, w, scales, bias, y, M, N, K, has_bias,
                                 x_tile, tid, simd_gid, lane_id, gid);
}

#if defined(__HAVE_BFLOAT__)
[[kernel]] void mxfp4_matmul_bf16(const device bfloat16_t *x [[buffer(0)]],
                                  const device uchar *w [[buffer(1)]],
                                  const device uchar *scales [[buffer(2)]],
                                  const device bfloat16_t *bias [[buffer(3)]],
                                  device bfloat16_t *y [[buffer(4)]],
                                  const constant int &M [[buffer(5)]],
                                  const constant int &N [[buffer(6)]],
                                  const constant int &K [[buffer(7)]],
                                  const constant int &has_bias [[buffer(8)]],
                                  uint tid [[thread_index_in_threadgroup]],
                                  ushort simd_gid
                                  [[simdgroup_index_in_threadgroup]],
                                  ushort lane_id [[thread_index_in_simdgroup]],
                                  uint3 gid [[threadgroup_position_in_grid]]) {
  threadgroup float x_tile[mxfp4::kKTilePadded];
  mxfp4::mxfp4_matmul_impl<bfloat16_t>(x, w, scales, bias, y, M, N, K, has_bias,
                                       x_tile, tid, simd_gid, lane_id, gid);
}
#endif

[[kernel]] void mxfp4_moe_gemm_split_f16(
    const device half *x [[buffer(0)]], const device uchar *w [[buffer(1)]],
    const device uchar *scales [[buffer(2)]],
    const device half *biases [[buffer(3)]],
    const device uint *indices [[buffer(4)]],
    const device float *topk_weights [[buffer(5)]],
    device half *y [[buffer(6)]],
    const constant int &num_tokens [[buffer(7)]],
    const constant int &topk [[buffer(8)]],
    const constant int &num_experts [[buffer(9)]],
    const constant int &N [[buffer(10)]], const constant int &K [[buffer(11)]],
    const constant int &has_bias [[buffer(12)]],
    const constant int &input_has_topk_dim [[buffer(13)]],
    const constant int &has_topk_weights [[buffer(14)]],
    uint tid [[thread_index_in_threadgroup]],
    ushort simd_gid [[simdgroup_index_in_threadgroup]],
    ushort lane_id [[thread_index_in_simdgroup]],
    uint3 gid [[threadgroup_position_in_grid]]) {
  threadgroup float x_tile[mxfp4::kKTilePadded];
  mxfp4::mxfp4_moe_gemm_split_impl<half>(
      x, w, scales, biases, indices, topk_weights, y, num_tokens, topk,
      num_experts, N, K, has_bias, input_has_topk_dim, has_topk_weights,
      x_tile, tid, simd_gid, lane_id, gid);
}

#if defined(__HAVE_BFLOAT__)
[[kernel]] void
mxfp4_moe_gemm_split_bf16(const device bfloat16_t *x [[buffer(0)]],
                          const device uchar *w [[buffer(1)]],
                          const device uchar *scales [[buffer(2)]],
                          const device bfloat16_t *biases [[buffer(3)]],
                          const device uint *indices [[buffer(4)]],
                          const device float *topk_weights [[buffer(5)]],
                          device bfloat16_t *y [[buffer(6)]],
                          const constant int &num_tokens [[buffer(7)]],
                          const constant int &topk [[buffer(8)]],
                          const constant int &num_experts [[buffer(9)]],
                          const constant int &N [[buffer(10)]],
                          const constant int &K [[buffer(11)]],
                          const constant int &has_bias [[buffer(12)]],
                          const constant int &input_has_topk_dim [[buffer(13)]],
                          const constant int &has_topk_weights [[buffer(14)]],
                          uint tid [[thread_index_in_threadgroup]],
                          ushort simd_gid [[simdgroup_index_in_threadgroup]],
                          ushort lane_id [[thread_index_in_simdgroup]],
                          uint3 gid [[threadgroup_position_in_grid]]) {
  threadgroup float x_tile[mxfp4::kKTilePadded];
  mxfp4::mxfp4_moe_gemm_split_impl<bfloat16_t>(
      x, w, scales, biases, indices, topk_weights, y, num_tokens, topk,
      num_experts, N, K, has_bias, input_has_topk_dim, has_topk_weights,
      x_tile, tid, simd_gid, lane_id, gid);
}
#endif

[[kernel]] void mxfp4_moe_gemm_reuse_f16(
    const device half *x [[buffer(0)]], const device uchar *w [[buffer(1)]],
    const device uchar *scales [[buffer(2)]],
    const device half *biases [[buffer(3)]],
    const device uint *indices [[buffer(4)]],
    const device float *topk_weights [[buffer(5)]],
    device half *y [[buffer(6)]],
    const constant int &num_tokens [[buffer(7)]],
    const constant int &topk [[buffer(8)]],
    const constant int &num_experts [[buffer(9)]],
    const constant int &N [[buffer(10)]], const constant int &K [[buffer(11)]],
    const constant int &has_bias [[buffer(12)]],
    const constant int &has_topk_weights [[buffer(13)]],
    uint tid [[thread_index_in_threadgroup]],
    ushort simd_gid [[simdgroup_index_in_threadgroup]],
    ushort lane_id [[thread_index_in_simdgroup]],
    uint3 gid [[threadgroup_position_in_grid]]) {
  threadgroup float x_tile[mxfp4::kKTilePadded];
  mxfp4::mxfp4_moe_gemm_reuse_impl<half, 8>(
      x, w, scales, biases, indices, topk_weights, y, num_tokens, topk,
      num_experts, N, K, has_bias, has_topk_weights, tid, simd_gid, x_tile,
      lane_id, gid);
}

#if defined(__HAVE_BFLOAT__)
[[kernel]] void
mxfp4_moe_gemm_reuse_bf16(const device bfloat16_t *x [[buffer(0)]],
                          const device uchar *w [[buffer(1)]],
                          const device uchar *scales [[buffer(2)]],
                          const device bfloat16_t *biases [[buffer(3)]],
                          const device uint *indices [[buffer(4)]],
                          const device float *topk_weights [[buffer(5)]],
                          device bfloat16_t *y [[buffer(6)]],
                          const constant int &num_tokens [[buffer(7)]],
                          const constant int &topk [[buffer(8)]],
                          const constant int &num_experts [[buffer(9)]],
                          const constant int &N [[buffer(10)]],
                          const constant int &K [[buffer(11)]],
                          const constant int &has_bias [[buffer(12)]],
                          const constant int &has_topk_weights [[buffer(13)]],
                          uint tid [[thread_index_in_threadgroup]],
                          ushort simd_gid [[simdgroup_index_in_threadgroup]],
                          ushort lane_id [[thread_index_in_simdgroup]],
                          uint3 gid [[threadgroup_position_in_grid]]) {
  threadgroup float x_tile[mxfp4::kKTilePadded];
  mxfp4::mxfp4_moe_gemm_reuse_impl<bfloat16_t, 8>(
      x, w, scales, biases, indices, topk_weights, y, num_tokens, topk,
      num_experts, N, K, has_bias, has_topk_weights, tid, simd_gid, x_tile,
      lane_id, gid);
}
#endif
