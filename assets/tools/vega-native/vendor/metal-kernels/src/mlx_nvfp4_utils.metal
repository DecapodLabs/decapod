#include "metal_dtype.metal"
#include <metal_stdlib>

using namespace metal;

// MLX NVFP4 utility kernels for Metal.
//
// 1. mlx_nvfp4_repack_u32_to_u8: Reinterpret U32 packed weights as U8 bytes.
// 2. mlx_nvfp4_dequant_embedding: Dequantize MLX NVFP4 embeddings to F16/BF16.

namespace mlx_nvfp4 {

// FP4 E2M1 dequant LUT
constant float kFp4Lut[16] = {
    0.0f, 0.5f, 1.0f, 1.5f, 2.0f, 3.0f, 4.0f, 6.0f,
    -0.0f, -0.5f, -1.0f, -1.5f, -2.0f, -3.0f, -4.0f, -6.0f
};

// FP8 E4M3 → float (matches CUDA exactly)
METAL_FUNC float fp8e4m3_to_float(uchar bits) {
    uint sign = (bits >> 7) & 1;
    uint exp  = (bits >> 3) & 0xF;
    uint mant = bits & 0x7;

    if (exp == 0) {
        if (mant == 0) {
            return as_type<float>(sign << 31);
        }
        float result = float(mant) * 0.001953125f;
        return sign ? -result : result;
    }
    if (exp == 0xF && mant == 0x7) {
        return 0.0f;
    }
    uint new_exp = exp - 7 + 127;
    uint mant32 = uint(mant) << (23 - 3);
    uint fbits = (sign << 31) | (new_exp << 23) | mant32;
    return as_type<float>(fbits);
}

// Kernel 1: Repack U32 → U8
kernel void mlx_nvfp4_repack_u32_to_u8_kernel(
    device const uint32_t* input [[buffer(0)]],
    device uint8_t* output [[buffer(1)]],
    constant uint& num_rows [[buffer(2)]],
    constant uint& num_u32_cols [[buffer(3)]],
    uint tid [[thread_position_in_grid]])
{
    uint total = num_rows * num_u32_cols;
    if (tid >= total) return;

    uint32_t val = input[tid];
    uint out_base = tid * 4;
    output[out_base + 0] = uint8_t(val & 0xFF);
    output[out_base + 1] = uint8_t((val >> 8) & 0xFF);
    output[out_base + 2] = uint8_t((val >> 16) & 0xFF);
    output[out_base + 3] = uint8_t((val >> 24) & 0xFF);
}

// Kernel 2: Dequant embedding
template <typename T>
kernel void mlx_nvfp4_dequant_embedding_kernel(
    device const uint32_t* weight_u32 [[buffer(0)]],
    device const uint8_t* scales [[buffer(1)]],
    device T* output [[buffer(2)]],
    constant uint& vocab_size [[buffer(3)]],
    constant uint& hidden_size [[buffer(4)]],
    uint tid [[thread_position_in_grid]])
{
    uint bytes_per_row = hidden_size / 2;
    uint u32_per_row = hidden_size / 8;
    uint scale_per_row = hidden_size / 16;

    uint total = vocab_size * bytes_per_row;
    if (tid >= total) return;

    uint row = tid / bytes_per_row;
    uint byte_in_row = tid % bytes_per_row;

    uint u32_idx = byte_in_row / 4;
    uint byte_within = byte_in_row % 4;

    uint32_t packed = weight_u32[row * u32_per_row + u32_idx];
    uint8_t byte_val = uint8_t((packed >> (byte_within * 8)) & 0xFF);

    uint8_t lo = byte_val & 0xF;
    uint8_t hi = (byte_val >> 4) & 0xF;

    uint col = byte_in_row * 2;
    uint lo_si = col / 16;
    uint hi_si = (col + 1) / 16;
    float lo_scale = fp8e4m3_to_float(scales[row * scale_per_row + lo_si]);
    float hi_scale = fp8e4m3_to_float(scales[row * scale_per_row + hi_si]);

    float lo_val = kFp4Lut[lo] * lo_scale;
    float hi_val = kFp4Lut[hi] * hi_scale;

    uint off = row * hidden_size + col;
    output[off]     = T(lo_val);
    output[off + 1] = T(hi_val);
}

template [[host_name("mlx_nvfp4_dequant_embedding_f16")]]
kernel void mlx_nvfp4_dequant_embedding_kernel<half>(
    device const uint32_t*, device const uint8_t*, device half*,
    constant uint&, constant uint&, uint);

template [[host_name("mlx_nvfp4_dequant_embedding_bf16")]]
kernel void mlx_nvfp4_dequant_embedding_kernel<bfloat16_t>(
    device const uint32_t*, device const uint8_t*, device bfloat16_t*,
    constant uint&, constant uint&, uint);

}  // namespace mlx_nvfp4
