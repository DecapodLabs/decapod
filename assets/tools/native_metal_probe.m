// Reviewed native capability proof, independent of the candidate Vega runtime.
// Compile only on Apple Silicon. A runner label or device name is not proof.
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <math.h>
#include <stdio.h>
#include <unistd.h>

static int fail(NSString *reason) {
    fprintf(stderr, "native Metal capability unavailable: %s\n", reason.UTF8String);
    return 1;
}

int main(int argc, const char *argv[]) {
#if !defined(__APPLE__) || !defined(__arm64__)
    return fail(@"native Apple Silicon is required");
#else
    @autoreleasepool {
        if (argc != 2) return fail(@"expected one new evidence file");
        // Hard process bound even if a GPU command or compiler hangs.
        alarm(60);
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (!device || !device.hasUnifiedMemory || device.isRemovable)
            return fail(@"an integrated Apple GPU with unified memory is required");
        NSError *error = nil;
        NSString *source = @"#include <metal_stdlib>\nusing namespace metal;\n"
            "kernel void proof(device const float *a [[buffer(0)]], "
            "device const float *b [[buffer(1)]], device float *out [[buffer(2)]], "
            "uint i [[thread_position_in_grid]]) { out[i] = a[i] * 3.0f + b[i]; }\n";
        MTLCompileOptions *options = [MTLCompileOptions new];
        options.fastMathEnabled = NO;
        id<MTLLibrary> library = [device newLibraryWithSource:source options:options error:&error];
        if (!library) return fail(error.localizedDescription ?: @"shader compilation failed");
        id<MTLFunction> function = [library newFunctionWithName:@"proof"];
        if (!function) return fail(@"compiled kernel is absent");
        id<MTLComputePipelineState> pipeline = [device newComputePipelineStateWithFunction:function error:&error];
        if (!pipeline) return fail(error.localizedDescription ?: @"pipeline compilation failed");
        const float a[] = {1.0f, -2.0f, 3.5f, 0.0f, 100.0f, -0.25f, 7.0f, 19.0f};
        const float b[] = {4.0f, 9.0f, -0.5f, 8.0f, -13.0f, 0.75f, 2.0f, -7.0f};
        const float expected[] = {7.0f, 3.0f, 10.0f, 8.0f, 287.0f, 0.0f, 23.0f, 50.0f};
        const NSUInteger count = sizeof(a) / sizeof(a[0]);
        float poison[count];
        for (NSUInteger i = 0; i < count; ++i) poison[i] = NAN;
        id<MTLBuffer> left = [device newBufferWithBytes:a length:sizeof(a) options:MTLResourceStorageModeShared];
        id<MTLBuffer> right = [device newBufferWithBytes:b length:sizeof(b) options:MTLResourceStorageModeShared];
        id<MTLBuffer> output = [device newBufferWithBytes:poison length:sizeof(poison) options:MTLResourceStorageModeShared];
        id<MTLCommandQueue> queue = [device newCommandQueue];
        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
        if (!left || !right || !output || !queue || !command || !encoder)
            return fail(@"Metal allocation or command creation failed");
        [encoder setComputePipelineState:pipeline];
        [encoder setBuffer:left offset:0 atIndex:0];
        [encoder setBuffer:right offset:0 atIndex:1];
        [encoder setBuffer:output offset:0 atIndex:2];
        [encoder dispatchThreads:MTLSizeMake(count, 1, 1)
            threadsPerThreadgroup:MTLSizeMake(MIN(count, pipeline.maxTotalThreadsPerThreadgroup), 1, 1)];
        [encoder endEncoding];
        [command commit];
        [command waitUntilCompleted];
        if (command.status != MTLCommandBufferStatusCompleted || command.error)
            return fail(command.error.localizedDescription ?: @"GPU command did not complete");
        const float *actual = output.contents;
        NSMutableArray *values = [NSMutableArray array];
        for (NSUInteger i = 0; i < count; ++i) {
            if (!isfinite(actual[i]) || actual[i] != expected[i])
                return fail(@"GPU numerical result differs from the fixed expected vector");
            [values addObject:@(actual[i])];
        }
        NSDictionary *evidence = @{
            @"schema": @"decapod.native-metal-capability.v1",
            @"device": device.name,
            @"registry_id": @(device.registryID),
            @"unified_memory": @(device.hasUnifiedMemory),
            @"backend": @"metal",
            @"native_architecture": @"arm64",
            @"command_status": @"completed",
            @"operation": @"a * 3 + b",
            @"elements": @(count),
            @"output": values,
            @"numerical_result_verified": @YES,
            @"cpu_fallback": @NO
        };
        NSData *json = [NSJSONSerialization dataWithJSONObject:evidence options:NSJSONWritingSortedKeys error:&error];
        if (!json || ![json writeToFile:[NSString stringWithUTF8String:argv[1]]
                options:NSDataWritingWithoutOverwriting error:&error])
            return fail(error.localizedDescription ?: @"cannot write fresh GPU evidence");
        fwrite(json.bytes, 1, json.length, stdout);
        fputc('\n', stdout);
        alarm(0);
        return 0;
    }
#endif
}
