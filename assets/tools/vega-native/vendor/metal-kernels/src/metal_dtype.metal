#include <metal_stdlib>
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
inline void from_float(thread Float8_& dst, Float8_ src) {
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

#define MIN_FP8_VALUE (-447.0f) // -max representable normal fp8 E4M3
#define MAX_FP8_VALUE (447.0f)  // max representable normal fp8 E4M3

// FP8 E4M3 conversion
static inline uint as_bits(float x) { return as_type<uint>(x); }
static inline float from_bits(uint b) { return as_type<float>(b); }


inline float softmax_fp8_to_float(uint8_t v) {
  // Fast path for normal numbers (Exponent 1..15)
  // E4M3FN: 1 Sign, 4 Exp, 3 Mant. Bias 7.
  // FP32:   1 Sign, 8 Exp, 23 Mant. Bias 127.
  
  const uint8_t s_mask = 0x80;
  const uint8_t e_mask = 0x78;
  const uint8_t m_mask = 0x07;
  
  uint8_t exp8 = (v & e_mask) >> 3;
  uint8_t man8 = (v & m_mask);
  
  if (exp8 > 0) {
      // Normals (inc. Exp=15 which is normal in E4M3FN unless mant=7)
      if (exp8 == 0xF && man8 == 0x7) return NAN; // NaN
      
      uint32_t sign = (v & s_mask) << 24;
      // New Exp = OldExp - 7 + 127 = OldExp + 120
      uint32_t exp32 = (uint32_t(exp8) + 120) << 23;
      // New Mant = OldMant << 20 (align 3 bits to top of 23)
      uint32_t man32 = uint32_t(man8) << 20;
      
      return as_type<float>(sign | exp32 | man32);
  }
  
  // Subnormals (Exp=0) & Zero
  if (man8 == 0) {
      return (v & s_mask) ? -0.0f : 0.0f;
  }
  
  // Denorm fallback (computation)
  const float m = float(man8) / 8.f; 
  float val = ldexp(m, -6); // 2^(1-7) = -6
  return (v & s_mask) ? -val : val;
}

inline uint8_t float_to_softmax_fp8(float f) {
  // bit-level access
  uint bits = as_bits(f);
  int sign = (bits >> 31) & 1;
  int exp = (bits >> 23) & 0xFF;
  uint mant = bits & 0x7FFFFF;

  // special cases
  if (exp == 0xFF) { // Inf or NaN
    if (mant != 0) {
      // NaN: exp=1111, mant=111 (E4M3FN reserves only this for NaN)
      // 0x7F (Positive NaN) or 0xFF (Negative NaN)
      return (uchar)((0 << 7) | (0xF << 3) | 0x7);
    } else {
      // Inf -> saturate to max finite (E4M3FN has no Inf)
      // Max finite is Exp=15, Mant=6 (+/- 448)
      return (uchar)((sign << 7) | (0xF << 3) | 0x6);
    }
  }

  // zero
  if (exp == 0 && mant == 0) {
    return (uchar)(sign << 7);
  }

  // normalized float: compute new exponent with bias differences
  const int FP32_BIAS = 127;
  const int FP8_BIAS = 7;
  int new_exp = exp - FP32_BIAS + FP8_BIAS;

  // Prepare mantissa including hidden 1 for normals
  uint mant_with_hidden = mant;
  if (exp != 0) {
    mant_with_hidden |= (1u << 23);
  } else {
    // subnormal float -- handled below
  }

  // Overflow -> saturate to max finite (E4M3FN)
  // Max exp is 15. If new_exp > 15, saturate.
  if (new_exp > 0xF) {
    return (uchar)((sign << 7) | (0xF << 3) | 0x6);
  }

  // Underflow -> could become subnormal or zero in FP8
  if (new_exp <= 0) {
    // shift needed to form subnormal in FP8
    // effective shift: number of bits to right-shift mantissa to align to 3-bit mantissa
    int shift = (1 - new_exp) + (23 - 3); // (1-new_exp) accounts for denorm scaling
    if (shift >= 32) {
      // too small -> underflow to zero
      return (uchar)(sign << 7);
    }
    // build a rounding window
    uint abs_mant = mant_with_hidden;
    // Round-to-nearest-even for subnormal:
    uint truncated = abs_mant >> shift;
    uint remainder = abs_mant & ((1u << shift) - 1u);

    // rounding decision: look at the highest discarded bit and lower bits
    uint halfs = 1u << (shift - 1);
    if ( (remainder > halfs) || (remainder == halfs && (truncated & 1u)) ) {
      truncated += 1u;
    }

    // truncated now contains the 3-bit (or fewer) significand; clamp to 3 bits
    uint mant8 = truncated & 0x7u;
    if (mant8 == 0) {
      // rounds to zero
      return (uint8_t)(sign << 7);
    } else {
      // exp = 0 (subnormal)
      return (uint8_t)((sign << 7) | (0 << 3) | (mant8));
    }
  }

  // Normal case: need to round mantissa to 3 bits
  // shift mantissa (23 -> 3): keep top 3 bits of mant_with_hidden (which has 24 bits)
  int shift_norm = 23 - 3;
  uint truncated = (mant_with_hidden >> shift_norm) & 0x7u; // 3 bits
  // gather bits used for rounding
  uint round_bit_pos = shift_norm - 1;
  uint round_bit = (mant_with_hidden >> round_bit_pos) & 1u;
  uint sticky_mask = (1u << round_bit_pos) - 1u;
  uint sticky = (mant_with_hidden & sticky_mask) ? 1u : 0u;

  // Round-to-nearest-even
  if (round_bit && (sticky || (truncated & 1u))) {
    truncated += 1u;
    if (truncated == (1u << 3)) {
      // mantissa overflow -> increment exponent
      truncated = 0;
      new_exp += 1;
      if (new_exp > 0xF) {
        // overflow to max finite
        return (uchar)((sign << 7) | (0xF << 3) | 0x6);
      }
    }
  }

  // E4M3FN Check:
  // If we rounded up to Exp=15, Mant=7 (NaN), we should saturate to Max Finite?
  // Or is it actually strict?
  // Spec says operations that overflow saturate. Rounding 447.5 to 448 is Valid (Exp=15, Man=6).
  // Rounding to > 448 -> Max Finite.
  // Exp=15, Mant=7 is "NaN".
  if (new_exp == 0xF && truncated == 0x7) {
      truncated = 0x6; // Clamp to max finite
  }

  uchar out = (uchar)((sign << 7) | ((new_exp & 0xF) << 3) | (truncated & 0x7));
  return out;
}

// inline uint8_t float_to_softmax_fp8(float f) {
//   const int EXP_BITS = 4;
//   const int MAN_BITS = 3;
//   const int BIAS = 7;

//   const uint bits = as_bits(f);
//   const uint s = bits >> 31;
//   const uint abs = bits & 0x7FFFFFFF;

//   // NaN propagates, Inf saturates
//   if (abs >= 0x7F800000u) {
//     return uchar((s << 7) | (((1u << EXP_BITS) - 1u) << MAN_BITS) |
//                  (abs != 0x7F800000u));
//   }

//   int e = int((abs >> 23) & 0xFF) - 127;   // unbiased exponent
//   uint m = abs & 0x7FFFFFu;                // 23-bit mantissa
//   const int EXP_MAX = (1 << EXP_BITS) - 2; // last finite exponent

//   // ---------- Normal path -------------------------------------------------
//   int e_fp8 = e + BIAS;
//   if (e_fp8 >= 1 && e_fp8 <= EXP_MAX) {
//     // round-to-nearest-even
//     const int shift = 23 - MAN_BITS;
//     uint mant = m >> shift;
//     const uint lsb = mant & 1u;
//     const uint round = (m >> (shift - 1)) & 1u;
//     const uint sticky = (m & ((1u << (shift - 1)) - 1u)) != 0u;
//     mant += (round & (sticky | lsb));
//     if (mant >> MAN_BITS) { // mantissa overflow
//       mant = 0;
//       ++e_fp8;
//       if (e_fp8 > EXP_MAX)
//         return uchar((s << 7) | (((1u << EXP_BITS) - 1u) << MAN_BITS)); // ∞
//     }
//     return uchar((s << 7) | (uint(e_fp8) << MAN_BITS) |
//                  (mant & ((1u << MAN_BITS) - 1u)));
//   }

//   // ---------- Sub-normal / under-flow ------------------------------------
//   if (e_fp8 < 1 - MAN_BITS) // too small -> ±0
//     return uchar(s << 7);

//   // shift so that exponent becomes 1
//   int rshift = (1 - e_fp8) + (23 - MAN_BITS);
//   uint mant = (0x800000u | m); // implicit 1
//   uint rounded = (mant + (1u << (rshift - 1))) >> rshift;
//   if (rounded == 0)
//     return uchar(s << 7); // rounds to zero

//   return uchar((s << 7) | (rounded & ((1u << MAN_BITS) - 1u)));
// }


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


template <typename Tout, typename Tin>
inline Tout scaled_vec_conversion(
    Tin x, float scale);

// fp8 -> half
template <>
inline half scaled_vec_conversion<half, uint8_t>(
    uint8_t a, float scale) {
  float f = softmax_fp8_to_float(a);
  return static_cast<half>(f * scale);
}

// fp8x2 -> half2
template <>
inline half2 scaled_vec_conversion<half2, uint16_t>(
    uint16_t a, float scale) {
  half2 ret;
  uint8_t b0 = (uint8_t)(a & 0xFFu);
  uint8_t b1 = (uint8_t)((a >> 8u) & 0xFFu);
  ret.x = static_cast<half>(softmax_fp8_to_float(b0) * scale);
  ret.y = static_cast<half>(softmax_fp8_to_float(b1) * scale);
  return ret;
}

// fp8x4 -> half2x2
template <>
inline half4 scaled_vec_conversion<half4, uint32_t>(
    uint32_t a, float scale) {
  half4 ret;
  half2 ar = scaled_vec_conversion<half2, uint16_t>((uint16_t)(a & 0xFFFFu), scale);
  half2 br = scaled_vec_conversion<half2, uint16_t>((uint16_t)((a >> 16u) & 0xFFFFu), scale);
  ret.x = ar.x;
  ret.y = ar.y;
  ret.z = br.x;
  ret.w = br.y;
  return ret;
}

// fp8x8 -> half2x4
template <>
inline Half8_ scaled_vec_conversion<Half8_, uint2>(
    uint2 a, float scale) {
  Half8_ ret;
  ret.x = scaled_vec_conversion<half4, uint32_t>(a.x, scale);
  ret.y = scaled_vec_conversion<half4, uint32_t>(a.y, scale);
  return ret;
}

// fp8 -> bfloat16
template <>
inline bfloat16_t scaled_vec_conversion<bfloat16_t, uint8_t>(
    uint8_t a, float scale) {
  float f = softmax_fp8_to_float(a);
  return static_cast<bfloat16_t>(f * scale);
}

// fp8x2 -> bfloat16x2
template <>
inline Bfloat2_ scaled_vec_conversion<Bfloat2_, uint16_t>(
    uint16_t a, float scale) {
  Bfloat2_ res;
  uint8_t b0 = (uint8_t)(a & 0xFFu);
  uint8_t b1 = (uint8_t)((a >> 8u) & 0xFFu);
  res.x = scaled_vec_conversion<bfloat16_t, uint8_t>(b0, scale);
  res.y = scaled_vec_conversion<bfloat16_t, uint8_t>(b1, scale);
  return res;
}

// fp8x4 -> bf16_4_t
template <>
inline Bfloat4_ scaled_vec_conversion<Bfloat4_, uint32_t>(
    uint32_t a, float scale) {
  Bfloat4_ res;
  res.x = scaled_vec_conversion<Bfloat2_, uint16_t>((uint16_t)(a & 0xFFFFu), scale);
  res.y = scaled_vec_conversion<Bfloat2_, uint16_t>((uint16_t)((a >> 16u) & 0xFFFFu), scale);
  return res;
}

// fp8x8 -> bf16_8_t
template <>
inline Bfloat8_ scaled_vec_conversion<Bfloat8_, uint2>(
    uint2 a, float scale) {
  Bfloat8_ res;
  res.x = scaled_vec_conversion<Bfloat4_, uint32_t>(a.x, scale);
  res.y = scaled_vec_conversion<Bfloat4_, uint32_t>(a.y, scale);
  return res;
}

// fp8 -> float
template <>
inline float scaled_vec_conversion<float, uint8_t>(
    uint8_t a, float scale) {
  return softmax_fp8_to_float(a) * scale;
}

// fp8x2 -> float2
template <>
inline float2 scaled_vec_conversion<float2, uint16_t>(
    uint16_t a, float scale) {
  uint8_t b0 = (uint8_t)(a & 0xFFu);
  uint8_t b1 = (uint8_t)((a >> 8u) & 0xFFu);
  float2 res;
  res.x = scaled_vec_conversion<float, uint8_t>(b0, scale);
  res.y = scaled_vec_conversion<float, uint8_t>(b1, scale);
  return res;
}

// fp8x4 -> float4
template <>
inline float4 scaled_vec_conversion<float4, uint32_t>(
    uint32_t a, float scale) {
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
inline Float8_ scaled_vec_conversion<Float8_, uint2>(
    uint2 a, float scale) {
  Float8_ res;
  res.x = scaled_vec_conversion<float4, uint32_t>(a.x, scale);
  res.y = scaled_vec_conversion<float4, uint32_t>(a.y, scale);
  return res;
}

// half -> fp8
template <>
inline uint8_t scaled_vec_conversion<uint8_t, half>(
    half a, float scale) {
  float f = fmin(fmax(static_cast<float>(a) / scale, MIN_FP8_VALUE), MAX_FP8_VALUE);
  return float_to_softmax_fp8(f);
}

// bf16 -> fp8
template <>
inline uint8_t scaled_vec_conversion<uint8_t, bfloat16_t>(
    bfloat16_t a, float scale) {
  float f = fmin(fmax(static_cast<float>(a) / scale, MIN_FP8_VALUE), MAX_FP8_VALUE);
  return float_to_softmax_fp8(f);
}

// float -> fp8
template <>
inline uint8_t scaled_vec_conversion<uint8_t, float>(
    float a, float scale) {
  float f = fmin(fmax(a / scale, MIN_FP8_VALUE), MAX_FP8_VALUE);
  return float_to_softmax_fp8(f);
}


template <typename Tout, typename Tin>
inline Tout scaled_convert(Tin x, float scale) {
    return scaled_vec_conversion<Tout, Tin>(x, scale);
}