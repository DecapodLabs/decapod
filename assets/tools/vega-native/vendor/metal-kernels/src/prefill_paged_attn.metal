/**
 * @brief Metal kernel for chunked prefill attention with paged KV-cache.
 * Copyright (c) 2025, Guoqing Bao.  All rights reserved.
 * This kernel computes attention during the prefill stage (processing prompt tokens)
 * using a **paged key-value cache**. Each thread computes the output for a token/head pair.
 *
 * This Metal kernel is part of the xInfer (vLLM.rs) project:
 * https://github.com/guoqingbao/attention.rs/tree/main/src/metal-kernels/src/prefill_paged_attn.metal
 * Features:
 *  - Support Chunked Prefill (prefilled attention with kvcache)
 *  - Supports **paged KV-cache** (blocks of tokens stored in memory).
 *  - Handles **sliding window attention**.
 *  - Optionally applies **ALiBi positional bias**.
 *  - Uses **online softmax** for numerical stability.
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

#include <metal_stdlib>
#include <metal_compute>
#include <metal_simdgroup>

using namespace metal;

#if defined(__HAVE_BFLOAT__)

typedef bfloat bfloat16_t;

#else

/////////////////////////////////////////////////////////////////////////////
// Helpers
/////////////////////////////////////////////////////////////////////////////

constexpr METAL_FUNC uint16_t float_to_bfloat_bits(float x) {
  // Check for nan
  if ((as_type<uint32_t>(x) & ~_fp_encoding_traits<float>::sign_mask) >
      _fp_encoding_traits<float>::inf_mask) {
    return uint16_t(as_type<uint32_t>(0x7FC0));
  }
  // Take bits
  uint32_t float_bits = as_type<uint32_t>(x);

  // Round to nearest even
  float_bits += ((float_bits >> 16) & 1) + as_type<uint32_t>(0x7FFF);

  // Take upper 16 bits
  return float_bits >> 16;
}

constexpr METAL_FUNC float bfloat_bits_to_float(uint16_t x) {
  // Upper 16 bits are the data and lower 16 bits are 0s
  return as_type<float>((uint32_t)x << 16);
}

struct _MLX_BFloat16;

template <typename T>
static constexpr constant bool can_convert_to_bfloat =
    !is_same_v<T, _MLX_BFloat16> && is_convertible_v<T, float>;

template <typename T>
static constexpr constant bool can_convert_from_bfloat =
    !is_same_v<T, _MLX_BFloat16> && is_convertible_v<float, T>;

/////////////////////////////////////////////////////////////////////////////
// Bfloat struct
/////////////////////////////////////////////////////////////////////////////

struct _MLX_BFloat16 {
  /////////////////////////////////////////////////////////////////////////////
  // Constructors
  uint16_t bits_;
  _MLX_BFloat16() thread = default;
  _MLX_BFloat16() threadgroup = default;
  _MLX_BFloat16() device = default;
  _MLX_BFloat16() constant = default;

  struct bits_to_bfloat_struct {};
  static constexpr METAL_FUNC bits_to_bfloat_struct bits_to_bfloat() {
    return bits_to_bfloat_struct();
  }
  constexpr METAL_FUNC _MLX_BFloat16(uint16_t bits, bits_to_bfloat_struct)
      : bits_(bits) {}

  /////////////////////////////////////////////////////////////////////////////
  // Conversions to bfloat

  template <
      typename T,
      typename = typename enable_if<can_convert_to_bfloat<T>>::type>
  constexpr METAL_FUNC _MLX_BFloat16(T x) thread
      : bits_(float_to_bfloat_bits(static_cast<float>(x))) {}

  template <
      typename T,
      typename = typename enable_if<can_convert_to_bfloat<T>>::type>
  constexpr METAL_FUNC _MLX_BFloat16(T x) threadgroup
      : bits_(float_to_bfloat_bits(static_cast<float>(x))) {}

  template <
      typename T,
      typename = typename enable_if<can_convert_to_bfloat<T>>::type>
  constexpr METAL_FUNC _MLX_BFloat16(T x) device
      : bits_(float_to_bfloat_bits(static_cast<float>(x))) {}

  template <
      typename T,
      typename = typename enable_if<can_convert_to_bfloat<T>>::type>
  constexpr METAL_FUNC _MLX_BFloat16(T x) constant
      : bits_(float_to_bfloat_bits(static_cast<float>(x))) {}

  /////////////////////////////////////////////////////////////////////////////
  // Conversions from bfloat

  template <
      typename T,
      typename = typename enable_if<can_convert_from_bfloat<T>>::type>
  constexpr METAL_FUNC operator T() const thread {
    return static_cast<T>(bfloat_bits_to_float(bits_));
  }

  template <
      typename T,
      typename = typename enable_if<can_convert_from_bfloat<T>>::type>
  constexpr METAL_FUNC operator T() const threadgroup {
    return static_cast<T>(bfloat_bits_to_float(bits_));
  }

  template <
      typename T,
      typename = typename enable_if<can_convert_from_bfloat<T>>::type>
  constexpr METAL_FUNC operator T() const device {
    return static_cast<T>(bfloat_bits_to_float(bits_));
  }

  template <
      typename T,
      typename = typename enable_if<can_convert_from_bfloat<T>>::type>
  constexpr METAL_FUNC operator T() const constant {
    return static_cast<T>(bfloat_bits_to_float(bits_));
  }
};

/////////////////////////////////////////////////////////////////////////////
// Bfloat operators
/////////////////////////////////////////////////////////////////////////////

/////////////////////////////////////////////////////////////////////////////
// Unary ops
constexpr METAL_FUNC _MLX_BFloat16 operator-(_MLX_BFloat16 x) {
  return -static_cast<float>(x);
}

/////////////////////////////////////////////////////////////////////////////
// Binary operators
#define bfloat_binop_base(__op__, __operator__, otype, atype, btype, ctype) \
  constexpr METAL_FUNC otype __operator__(atype lhs, btype rhs) {           \
    return static_cast<ctype>(lhs) __op__ static_cast<ctype>(rhs);          \
  }

#define bfloat_binop_helper(__op__, __operator__, otype, itype, ctype)    \
  constexpr METAL_FUNC otype __operator__(_MLX_BFloat16 lhs, itype rhs) { \
    return static_cast<ctype>(lhs) __op__ static_cast<ctype>(rhs);        \
  }                                                                       \
  constexpr METAL_FUNC otype __operator__(itype lhs, _MLX_BFloat16 rhs) { \
    return static_cast<ctype>(lhs) __op__ static_cast<ctype>(rhs);        \
  }

/////////////////////////////////////////////////////////////////////////////
// Arithmetic Operators
#define bfloat_binop(_op_, _operator_)                                       \
  bfloat_binop_base(                                                         \
      _op_, _operator_, _MLX_BFloat16, _MLX_BFloat16, _MLX_BFloat16, float); \
  bfloat_binop_helper(_op_, _operator_, float, float, float);                \
  bfloat_binop_helper(_op_, _operator_, float, half, float);                 \
  bfloat_binop_helper(_op_, _operator_, _MLX_BFloat16, int32_t, float);      \
  bfloat_binop_helper(_op_, _operator_, _MLX_BFloat16, uint32_t, float);     \
  bfloat_binop_helper(_op_, _operator_, _MLX_BFloat16, int64_t, float);      \
  bfloat_binop_helper(_op_, _operator_, _MLX_BFloat16, uint64_t, float);

bfloat_binop(+, operator+);
bfloat_binop(-, operator-);
bfloat_binop(*, operator*);
bfloat_binop(/, operator/);

/////////////////////////////////////////////////////////////////////////////
// Comparison ops
#define bfloat_compop(__op__, __operator__)                             \
  bfloat_binop_base(                                                    \
      __op__, __operator__, bool, _MLX_BFloat16, _MLX_BFloat16, float); \
  bfloat_binop_helper(__op__, __operator__, bool, float, float);        \
  bfloat_binop_helper(__op__, __operator__, bool, half, float);         \
  bfloat_binop_helper(__op__, __operator__, bool, int32_t, float);      \
  bfloat_binop_helper(__op__, __operator__, bool, uint32_t, float);     \
  bfloat_binop_helper(__op__, __operator__, bool, int64_t, float);      \
  bfloat_binop_helper(__op__, __operator__, bool, uint64_t, float);

bfloat_compop(>, operator>);
bfloat_compop(<, operator<);
bfloat_compop(>=, operator>=);
bfloat_compop(<=, operator<=);
bfloat_compop(==, operator==);
bfloat_compop(!=, operator!=);

#undef bfloat_compop
#undef bfloat_binop_base
#undef bfloat_binop_helper
#undef bfloat_binop

/////////////////////////////////////////////////////////////////////////////
// Inplace Operators
#define bfloat_inplace_op_helper(__op__, __operator__, itype, addr_space) \
  constexpr METAL_FUNC addr_space _MLX_BFloat16& __operator__(            \
      addr_space _MLX_BFloat16& lhs, itype rhs) {                         \
    lhs = static_cast<float>(lhs) __op__ static_cast<float>(rhs);         \
    return lhs;                                                           \
  }                                                                       \
  constexpr METAL_FUNC addr_space itype& __operator__(                    \
      addr_space itype& lhs, _MLX_BFloat16 rhs) {                         \
    lhs = static_cast<float>(lhs) __op__ static_cast<float>(rhs);         \
    return lhs;                                                           \
  }

#define bfloat_inplace_op_addr_space_helper(__op__, __operator__, itype) \
  bfloat_inplace_op_helper(__op__, __operator__, itype, device);         \
  bfloat_inplace_op_helper(__op__, __operator__, itype, thread);         \
  bfloat_inplace_op_helper(__op__, __operator__, itype, threadgroup);

#define bfloat_inplace_op(itype)                             \
  bfloat_inplace_op_addr_space_helper(+, operator+=, itype); \
  bfloat_inplace_op_addr_space_helper(-, operator-=, itype); \
  bfloat_inplace_op_addr_space_helper(*, operator*=, itype); \
  bfloat_inplace_op_addr_space_helper(/, operator/=, itype);

bfloat_inplace_op(float);
bfloat_inplace_op(half);
bfloat_inplace_op(int16_t);
bfloat_inplace_op(int32_t);
bfloat_inplace_op(int64_t);
bfloat_inplace_op(uint16_t);
bfloat_inplace_op(uint32_t);
bfloat_inplace_op(uint64_t);

#undef bfloat_inplace_op_helper
#undef bfloat_inplace_op_addr_space_helper
#undef bfloat_inplace_op

#define bfloat_inplace_op_helper(__op__, __operator__, addr_space) \
  constexpr METAL_FUNC addr_space _MLX_BFloat16& __operator__(     \
      addr_space _MLX_BFloat16& lhs, _MLX_BFloat16 rhs) {          \
    lhs = static_cast<float>(lhs) __op__ static_cast<float>(rhs);  \
    return lhs;                                                    \
  }

#define bfloat_inplace_op_addr_space_helper(__op__, __operator__) \
  bfloat_inplace_op_helper(__op__, __operator__, device);         \
  bfloat_inplace_op_helper(__op__, __operator__, thread);         \
  bfloat_inplace_op_helper(__op__, __operator__, threadgroup);

bfloat_inplace_op_addr_space_helper(+, operator+=);
bfloat_inplace_op_addr_space_helper(-, operator-=);
bfloat_inplace_op_addr_space_helper(*, operator*=);
bfloat_inplace_op_addr_space_helper(/, operator/=);

#undef bfloat_inplace_op_helper
#undef bfloat_inplace_op_addr_space_helper

/////////////////////////////////////////////////////////////////////////////
// Bfloat typedef
/////////////////////////////////////////////////////////////////////////////

typedef struct _MLX_BFloat16 bfloat16_t;

#endif

// ========================================== Generic vector types

// A vector type to store Q, K, V elements.
template<typename T, int VEC_SIZE>
struct Vec {};

// A vector type to store FP32 accumulators.
template<typename T>
struct FloatVec {};

// Template vector operations.
template<typename Acc, typename A, typename B>
inline Acc mul(A a, B b);

template<typename T>
inline float sum(T v);

template<typename T>
inline float dot(T a, T b) {
  return sum(mul<T, T, T>(a, b));
}

template<typename A, typename T>
inline float dot(T a, T b) {
  return sum(mul<A, T, T>(a, b));
}



// FP32 vector data types.
struct Float8_ {
  float4 x;
  float4 y;
};

template<>
struct Vec<float, 1> {
  using Type = float;
};
template<>
struct Vec<float, 2> {
  using Type = float2;
};
template<>
struct Vec<float, 4> {
  using Type = float4;
};
template<>
struct Vec<float, 8> {
  using Type = Float8_;
};

template<>
struct FloatVec<float> {
  using Type = float;
};
template<>
struct FloatVec<float2> {
  using Type = float2;
};
template<>
struct FloatVec<float4> {
  using Type = float4;
};
template<>
struct FloatVec<Float8_> {
  using Type = Float8_;
};

template<>
inline float mul(float a, float b) {
  return a*b;
}

template<>
inline float2 mul(float2 a, float2 b) {
  return a*b;
}

template<>
inline float4 mul(float4 a, float4 b) {
  return a*b;
}

template<>
inline Float8_ mul(Float8_ a, Float8_ b) {
  Float8_ c;
  c.x = a.x * b.x;
  c.y = a.y * b.y;
  return c;
}

template<>
inline float sum(float a) {
  return a;
}

template<>
inline float sum(float2 a) {
  return a.x + a.y;
}

template<>
inline float sum(float4 a) {
  return a.x + a.y + a.z + a.w;
}

template<>
inline float sum(Float8_ a) {
  return sum(a.x) + sum(a.y);
}

inline Float8_ fma(Float8_ a, Float8_ b, Float8_ c) {
  Float8_ res;
  res.x = fma(a.x, b.x, c.x);
  res.y = fma(a.y, b.y, c.y);
  return res;
}

inline void from_float(thread float& dst, float src) {
  dst = src;
}
inline void from_float(thread float2& dst, float2 src) {
  dst = src;
}
inline void from_float(thread float4& dst, float4 src) {
  dst = src;
}
inline void to_float(thread float4& dst, float4 src) {
  dst = src;
}
inline void from_float(thread Float8_& dst, Float8_ src) {
  dst = src;
}
inline void to_float(thread Float8_& dst, Float8_ src) {
  dst = src;
}


struct Bfloat2_ {
  bfloat16_t x;
  bfloat16_t y;
};

struct Bfloat4_ {
  Bfloat2_ x;
  Bfloat2_ y;
};

struct Bfloat8_ {
  Bfloat4_ x;
  Bfloat4_ y;
};

template<>
struct Vec<bfloat16_t, 1> {
  using Type = bfloat16_t;
};
template<>
struct Vec<bfloat16_t, 2> {
  using Type = Bfloat2_;
};
template<>
struct Vec<bfloat16_t, 4> {
  using Type = Bfloat4_;
};
template<>
struct Vec<bfloat16_t, 8> {
  using Type = Bfloat8_;
};

template<>
struct FloatVec<bfloat16_t> {
  using Type = float;
};
template<>
struct FloatVec<Bfloat2_> {
  using Type = float2;
};
template<>
struct FloatVec<Bfloat4_> {
  using Type = float4;
};
template<>
struct FloatVec<Bfloat8_> {
  using Type = Float8_;
};

template<>
inline float mul(bfloat16_t a, bfloat16_t b) {
  return (float)a * (float)b;
}
template<>
inline bfloat16_t mul(bfloat16_t a, bfloat16_t b) {
  return a*b;
}

template<>
inline float2 mul(Bfloat2_ a, Bfloat2_ b) {
  float2 a_f((float)a.x, (float)a.y);
  float2 b_f((float)b.x, (float)b.y);
  return a_f * b_f;
}
template<>
inline Bfloat2_ mul(Bfloat2_ a, Bfloat2_ b) {
  Bfloat2_ c;
  c.x = a.x * b.x;
  c.y = a.y * b.y;
  return c;
}

template<>
inline float4 mul(Bfloat4_ a, Bfloat4_ b) {
  float2 x = mul<float2, Bfloat2_, Bfloat2_>(a.x, b.x);
  float2 y = mul<float2, Bfloat2_, Bfloat2_>(a.y, b.y);
  float4 c;
  c.x = x.x;
  c.y = x.y;
  c.z = y.x;
  c.w = y.y;
  return c;
}
template<>
inline Bfloat4_ mul(Bfloat4_ a, Bfloat4_ b) {
  Bfloat4_ c;
  c.x = mul<Bfloat2_, Bfloat2_, Bfloat2_>(a.x, b.x);
  c.y = mul<Bfloat2_, Bfloat2_, Bfloat2_>(a.y, b.y);
  return c;
}

template<>
inline Float8_ mul(Bfloat8_ a, Bfloat8_ b) {
  Float8_ c;
  c.x = mul<float4, Bfloat4_, Bfloat4_>(a.x, b.x);
  c.y = mul<float4, Bfloat4_, Bfloat4_>(a.y, b.y);
  return c;
}
template<>
inline Bfloat8_ mul(Bfloat8_ a, Bfloat8_ b) {
  Bfloat8_ c;
  c.x = mul<Bfloat4_, Bfloat4_, Bfloat4_>(a.x, b.x);
  c.y = mul<Bfloat4_, Bfloat4_, Bfloat4_>(a.y, b.y);
  return c;
}

template<>
inline float sum(bfloat16_t a) {
  return (float)a;
}

template<>
inline float sum(Bfloat2_ a) {
  return (float)a.x + (float)a.y;
}

template<>
inline float sum(Bfloat4_ a) {
  return sum(a.x) + sum(a.y);
}

template<>
inline float sum(Bfloat8_ a) {
  return sum(a.x) + sum(a.y);
}

inline float fma(bfloat16_t a, bfloat16_t b, float c) {
  return (float)a * (float)b + c;
}
inline bfloat16_t fma(bfloat16_t a, bfloat16_t b, bfloat16_t c) {
  return a*b+c;
}

inline float2 fma(Bfloat2_ a, Bfloat2_ b, float2 c) {
  float2 a_f((float)a.x, (float)a.y);
  float2 b_f((float)b.x, (float)b.y);
  return a_f * b_f + c;
}
inline Bfloat2_ fma(Bfloat2_ a, Bfloat2_ b, Bfloat2_ c) {
  Bfloat2_ res;
  res.x = a.x * b.x + c.x;
  res.y = a.y * b.y + c.y;
  return res;
}

inline float4 fma(Bfloat4_ a, Bfloat4_ b, float4 c) {
  float4 res;
  res.x = fma(a.x.x, b.x.x, c.x);
  res.y = fma(a.x.y, b.x.y, c.y);
  res.z = fma(a.y.x, b.y.x, c.z);
  res.w = fma(a.y.y, b.y.y, c.w);
  return res;
}
inline Bfloat4_ fma(Bfloat4_ a, Bfloat4_ b, Bfloat4_ c) {
  Bfloat4_ res;
  res.x = fma(a.x, b.x, c.x);
  res.y = fma(a.y, b.y, c.y);
  return res;
}

inline Float8_ fma(Bfloat8_ a, Bfloat8_ b, Float8_ c) {
  float4 x = fma(a.x, b.x, c.x);
  float4 y = fma(a.y, b.y, c.y);
  Float8_ res;
  res.x = x;
  res.y = y;
  return res;
}
inline Bfloat8_ fma(Bfloat8_ a, Bfloat8_ b, Bfloat8_ c) {
  Bfloat8_ res;
  res.x = fma(a.x, b.x, c.x);
  res.y = fma(a.y, b.x, c.y);
  return c;
}

inline void from_float(thread bfloat16_t& dst, float src) {
  dst = static_cast<bfloat16_t>(src);
}
inline void from_float(thread Bfloat2_& dst, float2 src) {
  dst.x = static_cast<bfloat16_t>(src.x);
  dst.y = static_cast<bfloat16_t>(src.y);
}
inline void from_float(thread Bfloat4_& dst, float4 src) {
  dst.x.x = static_cast<bfloat16_t>(src.x);
  dst.x.y = static_cast<bfloat16_t>(src.y);
  dst.y.x = static_cast<bfloat16_t>(src.z);
  dst.y.y = static_cast<bfloat16_t>(src.w);
}
inline void from_float(thread Bfloat8_& dst, Float8_ src) {
  Bfloat4_ x;
  Bfloat4_ y;
  from_float(x, src.x);
  from_float(y, src.y);
  dst.x = x;
  dst.y = y;
}

inline void to_float(thread float4& dst, Bfloat4_ src) {
  dst.x = static_cast<float>(src.x.x);
  dst.y = static_cast<float>(src.x.y);
  dst.z = static_cast<float>(src.y.x);
  dst.w = static_cast<float>(src.y.y);
}

inline void to_float(thread Float8_& dst, Bfloat8_ src) {
  float4 x;
  float4 y;
  to_float(x, src.x);
  to_float(y, src.y);
  dst.x = x;
  dst.y = y;
}


// FP16 vector data types.
struct Half8_ {
  half4 x;
  half4 y;
};

template<>
struct Vec<half, 1> {
  using Type = half;
};
template<>
struct Vec<half, 2> {
  using Type = half2;
};
template<>
struct Vec<half, 4> {
  using Type = half4;
};
template<>
struct Vec<half, 8> {
  using Type = Half8_;
};

template<>
struct FloatVec<half> {
  using Type = float;
};
template<>
struct FloatVec<half2> {
  using Type = float2;
};
template<>
struct FloatVec<half4> {
  using Type = float4;
};
template<>
struct FloatVec<Half8_> {
  using Type = Float8_;
};

// FP8 vector data types (packed).
template<>
struct Vec<uint8_t, 1> {
  using Type = uint8_t;
};
template<>
struct Vec<uint8_t, 2> {
  using Type = uint16_t;
};
template<>
struct Vec<uint8_t, 4> {
  using Type = uint32_t;
};
template<>
struct Vec<uint8_t, 8> {
  using Type = uint2;
};

// FP8 E4M3FN -> float conversion.
static inline float softmax_fp8_to_float(uint8_t v) {
  const uint8_t s_mask = 0x80;
  const uint8_t e_mask = 0x78;
  const uint8_t m_mask = 0x07;

  uint8_t exp8 = (v & e_mask) >> 3;
  uint8_t man8 = (v & m_mask);

  if (exp8 > 0) {
    if (exp8 == 0xF && man8 == 0x7) return NAN;
    uint32_t sign = (v & s_mask) << 24;
    uint32_t exp32 = (uint32_t(exp8) + 120) << 23;
    uint32_t man32 = uint32_t(man8) << 20;
    return as_type<float>(sign | exp32 | man32);
  }

  if (man8 == 0) {
    return (v & s_mask) ? -0.0f : 0.0f;
  }

  const float m = float(man8) / 8.f;
  float val = ldexp(m, -6);
  return (v & s_mask) ? -val : val;
}

template <typename Tout, typename Tin>
inline Tout scaled_vec_conversion(Tin x, float scale);

// fp8 -> half
template <>
inline half scaled_vec_conversion<half, uint8_t>(uint8_t a, float scale) {
  float f = softmax_fp8_to_float(a);
  return static_cast<half>(f * scale);
}
// fp8x2 -> half2
template <>
inline half2 scaled_vec_conversion<half2, uint16_t>(uint16_t a, float scale) {
  half2 ret;
  uint8_t b0 = (uint8_t)(a & 0xFFu);
  uint8_t b1 = (uint8_t)((a >> 8u) & 0xFFu);
  ret.x = static_cast<half>(softmax_fp8_to_float(b0) * scale);
  ret.y = static_cast<half>(softmax_fp8_to_float(b1) * scale);
  return ret;
}
// fp8x4 -> half4
template <>
inline half4 scaled_vec_conversion<half4, uint32_t>(uint32_t a, float scale) {
  half4 ret;
  half2 ar = scaled_vec_conversion<half2, uint16_t>((uint16_t)(a & 0xFFFFu), scale);
  half2 br = scaled_vec_conversion<half2, uint16_t>((uint16_t)((a >> 16u) & 0xFFFFu), scale);
  ret.x = ar.x;
  ret.y = ar.y;
  ret.z = br.x;
  ret.w = br.y;
  return ret;
}
// fp8x8 -> half8
template <>
inline Half8_ scaled_vec_conversion<Half8_, uint2>(uint2 a, float scale) {
  Half8_ ret;
  ret.x = scaled_vec_conversion<half4, uint32_t>(a.x, scale);
  ret.y = scaled_vec_conversion<half4, uint32_t>(a.y, scale);
  return ret;
}

// fp8 -> bfloat16
template <>
inline bfloat16_t scaled_vec_conversion<bfloat16_t, uint8_t>(uint8_t a, float scale) {
  float f = softmax_fp8_to_float(a);
  return static_cast<bfloat16_t>(f * scale);
}
// fp8x2 -> bfloat2
template <>
inline Bfloat2_ scaled_vec_conversion<Bfloat2_, uint16_t>(uint16_t a, float scale) {
  Bfloat2_ res;
  uint8_t b0 = (uint8_t)(a & 0xFFu);
  uint8_t b1 = (uint8_t)((a >> 8u) & 0xFFu);
  res.x = scaled_vec_conversion<bfloat16_t, uint8_t>(b0, scale);
  res.y = scaled_vec_conversion<bfloat16_t, uint8_t>(b1, scale);
  return res;
}
// fp8x4 -> bfloat4
template <>
inline Bfloat4_ scaled_vec_conversion<Bfloat4_, uint32_t>(uint32_t a, float scale) {
  Bfloat4_ res;
  res.x = scaled_vec_conversion<Bfloat2_, uint16_t>((uint16_t)(a & 0xFFFFu), scale);
  res.y = scaled_vec_conversion<Bfloat2_, uint16_t>((uint16_t)((a >> 16u) & 0xFFFFu), scale);
  return res;
}
// fp8x8 -> bfloat8
template <>
inline Bfloat8_ scaled_vec_conversion<Bfloat8_, uint2>(uint2 a, float scale) {
  Bfloat8_ res;
  res.x = scaled_vec_conversion<Bfloat4_, uint32_t>(a.x, scale);
  res.y = scaled_vec_conversion<Bfloat4_, uint32_t>(a.y, scale);
  return res;
}

// fp8 -> float
template <>
inline float scaled_vec_conversion<float, uint8_t>(uint8_t a, float scale) {
  return softmax_fp8_to_float(a) * scale;
}
// fp8x2 -> float2
template <>
inline float2 scaled_vec_conversion<float2, uint16_t>(uint16_t a, float scale) {
  uint8_t b0 = (uint8_t)(a & 0xFFu);
  uint8_t b1 = (uint8_t)((a >> 8u) & 0xFFu);
  float2 res;
  res.x = scaled_vec_conversion<float, uint8_t>(b0, scale);
  res.y = scaled_vec_conversion<float, uint8_t>(b1, scale);
  return res;
}
// fp8x4 -> float4
template <>
inline float4 scaled_vec_conversion<float4, uint32_t>(uint32_t a, float scale) {
  float4 res;
  float2 ar = scaled_vec_conversion<float2, uint16_t>((uint16_t)(a & 0xFFFFu), scale);
  float2 br = scaled_vec_conversion<float2, uint16_t>((uint16_t)((a >> 16u) & 0xFFFFu), scale);
  res.x = ar.x;
  res.y = ar.y;
  res.z = br.x;
  res.w = br.y;
  return res;
}
// fp8x8 -> float8
template <>
inline Float8_ scaled_vec_conversion<Float8_, uint2>(uint2 a, float scale) {
  Float8_ res;
  res.x = scaled_vec_conversion<float4, uint32_t>(a.x, scale);
  res.y = scaled_vec_conversion<float4, uint32_t>(a.y, scale);
  return res;
}

template<>
inline float mul(half a, half b) {
  return (float)a * (float)b;
}
template<>
inline half mul(half a, half b) {
  return a*b;
}

template<>
inline float2 mul(half2 a, half2 b) {
  return (float2)a * (float2)b;
}
template<>
inline half2 mul(half2 a, half2 b) {
  return a * b;
}

template<>
inline float4 mul(half4 a, half4 b) {
  return (float4)a * (float4)b;
}
template<>
inline half4 mul(half4 a, half4 b) {
  return a * b;
}

template<>
inline Float8_ mul(Half8_ a, Half8_ b) {
  float4 x = mul<float4, half4, half4>(a.x, b.x);
  float4 y = mul<float4, half4, half4>(a.y, b.y);
  Float8_ c;
  c.x = x;
  c.y = y;
  return c;
}
template<>
inline Half8_ mul(Half8_ a, Half8_ b) {
  Half8_ c;
  c.x = mul<half4, half4, half4>(a.x, b.x);
  c.y = mul<half4, half4, half4>(a.y, b.y);
  return c;
}

template<>
inline float sum(half a) {
  return (float)a;
}

template<>
inline float sum(half2 a) {
  return (float)a.x + (float)a.y;
}

template<>
inline float sum(half4 a) {
  return sum(a.x) + sum(a.y);
}

template<>
inline float sum(Half8_ a) {
  return sum(a.x) + sum(a.y);
}

inline float fma(half a, half b, float c) {
  return (float)a * (float)b + c;
}

inline float2 fma(half2 a, half2 b, float2 c) {
  return (float2)a * (float2)b + c;
}

inline float4 fma(half4 a, half4 b, float4 c) {
  return (float4)a * (float4)b + c;
}

inline Float8_ fma(Half8_ a, Half8_ b, Float8_ c) {
  float4 x = fma(a.x, b.x, c.x);
  float4 y = fma(a.y, b.y, c.y);
  Float8_ res;
  res.x = x;
  res.y = y;
  return res;
}
inline Half8_ fma(Half8_ a, Half8_ b, Half8_ c) {
  Half8_ res;
  res.x = fma(a.x, b.x, c.x);
  res.y = fma(a.y, b.x, c.y);
  return c;
}

inline void from_float(thread half& dst, float src) {
  dst = static_cast<half>(src);
}
inline void from_float(thread half2& dst, float2 src) {
  dst.x = static_cast<half>(src.x);
  dst.y = static_cast<half>(src.y);
}
inline void from_float(thread half4& dst, float4 src) {
  dst.x = static_cast<half>(src.x);
  dst.y = static_cast<half>(src.y);
  dst.z = static_cast<half>(src.z);
  dst.w = static_cast<half>(src.w);
}
inline void from_float(thread Half8_& dst, Float8_ src) {
  half4 x;
  half4 y;
  from_float(x, src.x);
  from_float(y, src.y);
  dst.x = x;
  dst.y = y;
}


inline void to_float(thread float4& dst, half4 src) {
  dst.x = static_cast<float>(src.x);
  dst.y = static_cast<float>(src.y);
  dst.z = static_cast<float>(src.z);
  dst.w = static_cast<float>(src.w);
}

inline void to_float(thread Float8_& dst, Half8_ src) {
  float4 x;
  float4 y;
  to_float(x, src.x);
  to_float(y, src.y);
  dst.x = x;
  dst.y = y;
}

// ========================================== Dot product utilities

template<int THREAD_GROUP_SIZE, typename Vec, int N>
inline float qk_dot_(const thread Vec (&q)[N], const thread Vec (&k)[N]) {
  // Compute the parallel products for Q*K^T (treat vector lanes separately).
  using A_vec = typename FloatVec<Vec>::Type;
  A_vec qk_vec = mul<A_vec, Vec, Vec>(q[0], k[0]);
#pragma unroll
  for (int ii = 1; ii < N; ++ii) {
    qk_vec = fma(q[ii], k[ii], qk_vec);
  }

  // Finalize the reduction across lanes.
  float qk = sum(qk_vec);
#pragma unroll
  for (int mask = THREAD_GROUP_SIZE / 2; mask >= 1; mask /= 2) {
    qk += simd_shuffle_xor(qk, mask);
  }
  return qk;
}

template<typename T, int THREAD_GROUP_SIZE>
struct Qk_dot {
  template<typename Vec, int N>
  static inline float dot(const thread Vec (&q)[N], const thread Vec (&k)[N]) {
    return qk_dot_<THREAD_GROUP_SIZE>(q, k);
  }
};

#define MAX(a, b) ((a) > (b) ? (a) : (b))
#define MIN(a, b) ((a) < (b) ? (a) : (b))
#define DIVIDE_ROUND_UP(a, b) (((a) + (b) - 1) / (b))
#define UINT32_MAX 0xFFFFFFFF

/**
 * @brief Optimized paged attention for the prefill (prompt processing) stage.
 *
 * This kernel processes a batch of new query tokens with **shared memory tiling**
 * for cooperative K/V block loading and **binary search** for O(log N) sequence lookup.
 * All threads in a threadgroup cooperatively load K/V tiles into threadgroup memory,
 * then each thread computes attention for its assigned token from fast local memory.
 *
 * @tparam T The data type of the input and output tensors (e.g., float, half).
 * @tparam cache_t The KV cache storage type (same as T, or uint8_t for FP8).
 * @tparam HEAD_SIZE The dimension of each attention head.
 * @tparam BLOCK_SIZE The number of tokens stored in each block of the KV cache.
 * @tparam TOKEN_CHUNK_SIZE The number of threads in a threadgroup.
 *
 * @author Guoqing Bao
 * This kernel is part of the xInfer (vLLM.rs) project
 *
 * Optimizations over the baseline:
 *  - Threadgroup Memory Tiling: Cooperative loading of K/V blocks reduces global memory bandwidth.
 *  - Binary Search: O(log N) sequence lookup instead of O(N) linear scan.
 *  - Proper Barrier Discipline: No `continue` before barriers; all threads hit barriers uniformly.
 *
 * @section Dispatch Logic
 * - **Threadgroup Size:** `(TOKEN_CHUNK_SIZE, 1, 1)`
 * - **Grid Size:** `(num_queries_per_kv, num_kv_heads, num_token_chunks)`
 **/
template<typename T, typename cache_t, int HEAD_SIZE, int BLOCK_SIZE, int TOKEN_CHUNK_SIZE>
[[kernel]] void chunked_prefill_paged_attention(
    // Buffer mappings
    device T* out [[buffer(0)]],                    // [num_all_tokens_new, num_query_heads, HEAD_SIZE]
    device const T* q [[buffer(1)]],                // [num_all_tokens_new, num_query_heads, HEAD_SIZE]
    device const cache_t* k_cache [[buffer(2)]],          // [num_k_blocks, num_kv_heads, HEAD_SIZE/x, BLOCK_SIZE, x]
    device const cache_t* v_cache [[buffer(3)]],          // [num_k_blocks, num_kv_heads, HEAD_SIZE, BLOCK_SIZE]
    const constant int& num_kv_heads [[buffer(4)]],
    const constant float& sm_scale [[buffer(5)]],
    device const uint32_t* block_tables [[buffer(6)]],
    device const uint32_t* seq_lens [[buffer(7)]],
    const constant int& block_table_stride [[buffer(8)]],
    const constant int& num_seqs [[buffer(9)]],
    const constant int& num_query_heads [[buffer(10)]],
    const constant int& num_query_tokens [[buffer(11)]],
    const constant float& softcapping [[buffer(12)]],
    const constant int& o_stride_tokens [[buffer(13)]],
    device const uint32_t* query_start_len [[buffer(14)]],
    device const float* alibi_slopes [[buffer(15)]],
    device const float* k_scales [[buffer(16)]],
    device const float* v_scales [[buffer(17)]],
    device const float* sinks [[buffer(18)]],
    const constant int& sliding_window [[buffer(19)]],
    const constant int& total_num_blocks [[buffer(20)]],
    const constant int& kv_block_stride [[buffer(21)]],
    const constant int& kv_head_stride [[buffer(22)]],
    // Threading grid attributes
    uint3 threadgroup_position_in_grid [[threadgroup_position_in_grid]],
    uint3 thread_position_in_threadgroup [[thread_position_in_threadgroup]]
) {
    // Constants and Type Definitions
    constexpr int THREAD_GROUP_SIZE = 1;
    constexpr int VEC_SIZE = 16 / sizeof(T);
    constexpr int NUM_VECS = HEAD_SIZE / VEC_SIZE;
    constexpr bool is_quantized = !is_same_v<T, cache_t>;

    // --- Thread and Grid Mapping ---
    const int tid = thread_position_in_threadgroup.x;
    const int lane = tid;

    const int qh_base_idx = threadgroup_position_in_grid.x;
    const int kv_head_idx = threadgroup_position_in_grid.y;
    const int token_chunk_idx = threadgroup_position_in_grid.z;

    const int chunk_start = token_chunk_idx * TOKEN_CHUNK_SIZE;
    const int token_start = chunk_start + lane;

    constexpr int NUM_BLOCK_VECS = BLOCK_SIZE / VEC_SIZE;

    const int num_queries_per_kv = num_query_heads / num_kv_heads;
    constexpr int X = 16 / sizeof(cache_t);
    const bool use_alibi = (alibi_slopes != nullptr);
    const bool use_sinks = (sinks != nullptr);

    const int64_t q_stride_tokens = (int64_t)num_query_heads * (int64_t)HEAD_SIZE;
    const int64_t q_stride_heads  = (int64_t)HEAD_SIZE;
    const int64_t o_stride_heads  = (int64_t)HEAD_SIZE;

    // --- Threadgroup Memory Layout ---
    // SeqInfo: sequence metadata computed by lane 0 and broadcast to all threads
    // KV tile: A single [HEAD_SIZE * BLOCK_SIZE] buffer used sequentially for K then V
    struct SeqInfo {
        int seq_idx;
        int num_blocks;
        int start_block_idx;
        int start_token_idx;
    };

    threadgroup SeqInfo shared_seq_info[1];
    threadgroup cache_t kv_smem[HEAD_SIZE * BLOCK_SIZE];

    // --- Lane 0: Binary search for sequence index, broadcast via threadgroup memory ---
    if (lane == 0) {
        int seq_idx = 0;
        // Binary search: O(log N) instead of O(N) linear scan
        if (chunk_start < (int)query_start_len[num_seqs] && chunk_start >= (int)query_start_len[0]) {
            int left = 0, right = num_seqs - 1;
            while (left <= right) {
                int mid = (left + right) / 2;
                if ((int)query_start_len[mid + 1] <= chunk_start) {
                    left = mid + 1;
                } else if ((int)query_start_len[mid] > chunk_start) {
                    right = mid - 1;
                } else {
                    seq_idx = mid;
                    break;
                }
            }
        }

        uint32_t seq_len_full = seq_lens[seq_idx];
        int num_blocks_local = (int)DIVIDE_ROUND_UP(seq_len_full, BLOCK_SIZE);

        int start_token_idx = 0;
        int start_block_idx = 0;
        if (sliding_window > 0 && sliding_window < (int)seq_len_full) {
            start_token_idx = (int)seq_len_full - sliding_window;
            start_block_idx = start_token_idx / BLOCK_SIZE;
        }

        shared_seq_info[0].seq_idx = seq_idx;
        shared_seq_info[0].num_blocks = num_blocks_local;
        shared_seq_info[0].start_block_idx = start_block_idx;
        shared_seq_info[0].start_token_idx = start_token_idx;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // All threads read the shared sequence info
    const int seq_idx = shared_seq_info[0].seq_idx;
    const int num_blocks = shared_seq_info[0].num_blocks;
    const int start_block_idx = shared_seq_info[0].start_block_idx;
    const int start_token_idx = shared_seq_info[0].start_token_idx;
    const uint32_t seq_len_full = seq_lens[seq_idx];
    device const uint32_t* block_table_for_seq = block_tables + (int64_t)seq_idx * (int64_t)block_table_stride;

    // Vector types
    using Q_vec = typename Vec<T, VEC_SIZE>::Type;
    using K_vec = typename Vec<T, VEC_SIZE>::Type;
    using Float_vec = typename Vec<float, VEC_SIZE>::Type;
    using Quant_vec = typename Vec<cache_t, VEC_SIZE>::Type;

    Q_vec q_vec[NUM_VECS];
    float qk_block[BLOCK_SIZE];

    const int query_head_idx = kv_head_idx * num_queries_per_kv + qh_base_idx;
    const bool head_active = (qh_base_idx < num_queries_per_kv) && (query_head_idx < num_query_heads);
    const bool lane_active = token_start < num_query_tokens;

    // Load Q
    const int64_t q_off = (int64_t)token_start * q_stride_tokens + (int64_t)query_head_idx * q_stride_heads;
    const int64_t o_off = (int64_t)token_start * (int64_t)o_stride_tokens + (int64_t)query_head_idx * o_stride_heads;

    if (head_active && lane_active) {
      #pragma unroll
      for (int k = 0; k < NUM_VECS; k++) {
        int d_base = k * VEC_SIZE;
        q_vec[k] = *reinterpret_cast<device const Q_vec*>(q + q_off + d_base);
      }
    }

    // Accumulators for output
    float acc_vec[HEAD_SIZE] = { 0.0f };
    float M = use_sinks && head_active && lane_active ? sinks[query_head_idx] : -FLT_MAX;
    float alibi = (use_alibi && head_active && lane_active) ? alibi_slopes[query_head_idx] : 0.0f;
    float L = 1.0f;

    constexpr int elems_per_block = HEAD_SIZE * BLOCK_SIZE;

    // --- Main Loop Over KV Blocks ---
    for (int blk = start_block_idx; blk < num_blocks; ++blk) {
        const uint32_t physical_block = block_table_for_seq[blk];
        const bool valid_block = (physical_block != UINT32_MAX) &&
                                 ((uint64_t)physical_block < (uint64_t)total_num_blocks);

        // --- PHASE 1: Load K and Compute Q*K ---
        if (valid_block) {
            const int64_t kv_base = (int64_t)physical_block * kv_block_stride + (int64_t)kv_head_idx * kv_head_stride;
            device const cache_t* k_src = k_cache + kv_base;
            for (int i = tid; i < elems_per_block; i += TOKEN_CHUNK_SIZE) {
                kv_smem[i] = k_src[i];
            }
        }

        // BARRIER 1: All threads finish loading K
        threadgroup_barrier(mem_flags::mem_threadgroup);

        const int block_in_full = blk * BLOCK_SIZE;
        bool in_contexts[BLOCK_SIZE];

        if (valid_block) {
            for (int b = 0; b < BLOCK_SIZE; ++b) {
                const int token_idx_in_full = block_in_full + b;

                bool in_context = (token_idx_in_full < (int)seq_len_full);
                bool in_window = (token_idx_in_full >= start_token_idx);
                in_contexts[b] = in_context && in_window;

                if (!in_context || !in_window || !lane_active) {
                    qk_block[b] = -FLT_MAX;
                } else {
                    K_vec k_vec_local[NUM_VECS];
                    #pragma unroll
                    for (int k = 0; k < NUM_VECS; k++) {
                      int d = k * VEC_SIZE;
                      int gy = d / X;
                      int gx = d % X;
                      int smem_idx = b * X + gy * (BLOCK_SIZE * X) + gx;
                      if constexpr (!is_quantized) {
                        k_vec_local[k] = *reinterpret_cast<threadgroup const K_vec*>(&kv_smem[smem_idx]);
                      } else {
                        Quant_vec fp8_k_vec = *reinterpret_cast<threadgroup const Quant_vec*>(&kv_smem[smem_idx]);
                        k_vec_local[k] = scaled_vec_conversion<K_vec, Quant_vec>(fp8_k_vec, k_scales[0]);
                      }
                    }

                    float qk = Qk_dot<T, THREAD_GROUP_SIZE>::dot(q_vec, k_vec_local) * sm_scale;
                    if (softcapping != 1.0f) {
                      qk = tanh(qk / softcapping) * softcapping;
                    }
                    if (use_alibi) {
                        qk += alibi * float(token_idx_in_full - ((int)seq_len_full - 1));
                    }
                    qk_block[b] = qk;
                }
            } // end b loop
        } // end valid_block

        // BARRIER 2: Ensure everyone has consumed K before overwriting with V
        threadgroup_barrier(mem_flags::mem_threadgroup);

        // --- PHASE 2: Load V and Compute P*V ---
        if (valid_block) {
            const int64_t kv_base = (int64_t)physical_block * kv_block_stride + (int64_t)kv_head_idx * kv_head_stride;
            device const cache_t* v_src = v_cache + kv_base;
            for (int i = tid; i < elems_per_block; i += TOKEN_CHUNK_SIZE) {
                kv_smem[i] = v_src[i];
            }
        }

        // BARRIER 3: Wait for V load
        threadgroup_barrier(mem_flags::mem_threadgroup);

        if (valid_block && head_active && lane_active) {
            // Online softmax
            float Smax = -FLT_MAX;
            #pragma unroll
            for (int b = 0; b < BLOCK_SIZE; ++b) Smax = max(Smax, qk_block[b]);

            const float m_j = max(M, Smax);
            const float alpha = exp(M - m_j);
            M = m_j;
            L = L * alpha;
            #pragma unroll
            for (int i = 0; i < HEAD_SIZE; ++i) acc_vec[i] *= alpha;

            float acc_lane = 0.0f;
            Float_vec p_vec[NUM_BLOCK_VECS];
            #pragma unroll
            for (int b = 0; b < BLOCK_SIZE; ++b) {
              if (in_contexts[b]) {
                  const float P = exp(qk_block[b] - M);
                  reinterpret_cast<thread float*>(&p_vec[b / VEC_SIZE])[b % VEC_SIZE] = P;
                  acc_lane += P;
              } else {
                  reinterpret_cast<thread float*>(&p_vec[b / VEC_SIZE])[b % VEC_SIZE] = 0.0f;
              }
            }

            Float_vec v;
            for (int k = 0; k < HEAD_SIZE; ++k) {
              threadgroup const cache_t* v_row_ptr = kv_smem + (int64_t)k * BLOCK_SIZE;
              for (int b = 0; b < NUM_BLOCK_VECS; b++) {
                threadgroup const cache_t* src = v_row_ptr + b * VEC_SIZE;
                if constexpr (!is_quantized) {
                  to_float(v, *reinterpret_cast<threadgroup const K_vec*>(src));
                } else {
                  Quant_vec fp8_v_vec = *reinterpret_cast<threadgroup const Quant_vec*>(src);
                  v = scaled_vec_conversion<Float_vec, Quant_vec>(fp8_v_vec, v_scales[0]);
                }
                acc_vec[k] += dot(p_vec[b], v);
              }
            }

            L += acc_lane;
        }

        // BARRIER 4: Ensure V is fully consumed before next block iteration overwrites it
        threadgroup_barrier(mem_flags::mem_threadgroup);
    } // end block loop

    using O_vec = typename Vec<T, VEC_SIZE>::Type;

    // --- Write output for the current token ---
    if (head_active && lane_active) {
      O_vec o_vec[NUM_VECS];
      #pragma unroll
      for (int k = 0; k < HEAD_SIZE; k++) {
        float outv = acc_vec[k] / (L + 1e-6f);
        from_float(reinterpret_cast<thread T*>(&o_vec[k / VEC_SIZE])[k % VEC_SIZE], outv);
      }
      #pragma unroll
      for (int k = 0; k < NUM_VECS; k++) {
        *reinterpret_cast<device O_vec*>(out + o_off + k * VEC_SIZE) = o_vec[k];
      }
    }
}

#define instantiate_prefill_attention_inner(type, cache_type, head_size, block_size, token_chunk_size, suffix) \
    template [[host_name("chunked_prefill_" #type suffix "_hs" #head_size "_bs" #block_size "_tcs" #token_chunk_size)]] \
    [[kernel]] void chunked_prefill_paged_attention<type, cache_type, head_size, block_size, token_chunk_size>( \
        device type* out [[buffer(0)]], \
        device const type* q [[buffer(1)]], \
        device const cache_type* k_cache [[buffer(2)]], \
        device const cache_type* v_cache [[buffer(3)]], \
        const constant int& num_kv_heads [[buffer(4)]], \
        const constant float& sm_scale [[buffer(5)]], \
        device const uint32_t* block_tables [[buffer(6)]], \
        device const uint32_t* seq_lens [[buffer(7)]], \
        const constant int& block_table_stride [[buffer(8)]], \
        const constant int& num_seqs [[buffer(9)]], \
        const constant int& num_query_heads [[buffer(10)]], \
        const constant int& num_query_tokens [[buffer(11)]], \
        const constant float& softcapping [[buffer(12)]], \
        const constant int& o_stride_tokens [[buffer(13)]], \
        device const uint32_t* query_start_len [[buffer(14)]], \
        device const float* alibi_slopes [[buffer(15)]], \
        device const float* k_scales [[buffer(16)]], \
        device const float* v_scales [[buffer(17)]], \
        device const float* sinks [[buffer(18)]], \
        const constant int& sliding_window [[buffer(19)]], \
        const constant int& total_num_blocks [[buffer(20)]], \
        const constant int& kv_block_stride [[buffer(21)]], \
        const constant int& kv_head_stride [[buffer(22)]], \
        uint3 threadgroup_position_in_grid [[threadgroup_position_in_grid]], \
        uint3 thread_position_in_threadgroup [[thread_position_in_threadgroup]]);

// Instantiate for various head sizes
#define instantiate_prefill_attention_heads(type, cache_type, block_size, token_chunk_size, suffix) \
    instantiate_prefill_attention_inner(type, cache_type, 64,  block_size, token_chunk_size, suffix) \
    instantiate_prefill_attention_inner(type, cache_type, 96,  block_size, token_chunk_size, suffix) \
    instantiate_prefill_attention_inner(type, cache_type, 128, block_size, token_chunk_size, suffix) \
    instantiate_prefill_attention_inner(type, cache_type, 192, block_size, token_chunk_size, suffix) \
    instantiate_prefill_attention_inner(type, cache_type, 256, block_size, token_chunk_size, suffix)

// Instantiate for various block sizes
#define instantiate_prefill_attention_block_size(type, cache_type, token_chunk_size, suffix) \
    instantiate_prefill_attention_heads(type, cache_type, 32, token_chunk_size, suffix) \
    instantiate_prefill_attention_heads(type, cache_type, 64, token_chunk_size, suffix)

// Main instantiation macro for a given type
#define instantiate_prefill_attention(type, cache_type, suffix) \
    instantiate_prefill_attention_block_size(type, cache_type, 64, suffix) // Using a fixed TOKEN_CHUNK_SIZE of 64

// Call the macros to generate kernels for different data types
instantiate_prefill_attention(float, float, "")
instantiate_prefill_attention(half, half, "")
instantiate_prefill_attention(bfloat16_t, bfloat16_t, "")

instantiate_prefill_attention(float, uint8_t, "_uint8_t")
instantiate_prefill_attention(half, uint8_t, "_uint8_t")
instantiate_prefill_attention(bfloat16_t, uint8_t, "_uint8_t")
