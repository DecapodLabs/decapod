#include <metal_stdlib>
using namespace metal;

template <typename T>
[[kernel]] void gptoss_swiglu(
    device const T *gate [[buffer(0)]],
    device const T *up [[buffer(1)]],
    device T *output [[buffer(2)]],
    device const uint &N [[buffer(3)]],
    device const float &alpha [[buffer(4)]],
    device const float &limit [[buffer(5)]],
    uint idx [[thread_position_in_grid]])
{
    if (idx >= N) return;

    float g = static_cast<float>(gate[idx]);
    float u = static_cast<float>(up[idx]);

    float gate_clamped = min(g, limit);
    float up_clamped = max(min(u, limit), -limit);
    float glu = gate_clamped * (1.0f / (1.0f + exp(-gate_clamped * alpha)));
    float result = (up_clamped + 1.0f) * glu;

    output[idx] = static_cast<T>(result);
}

#define instantiate_gptoss_swiglu(type) \
    template [[host_name("gptoss_swiglu_" #type)]] [[kernel]] void gptoss_swiglu<type>( \
        device const type *gate [[buffer(0)]], \
        device const type *up [[buffer(1)]], \
        device type *output [[buffer(2)]], \
        device const uint &N [[buffer(3)]], \
        device const float &alpha [[buffer(4)]], \
        device const float &limit [[buffer(5)]], \
        uint idx [[thread_position_in_grid]]);

instantiate_gptoss_swiglu(float);
instantiate_gptoss_swiglu(half);
#if defined(__HAVE_BFLOAT__)
instantiate_gptoss_swiglu(bfloat);
#endif
