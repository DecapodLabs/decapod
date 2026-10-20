use candle_core::{DType, MetalStorage};
use metal::{
    Buffer, ComputeCommandEncoderRef, ComputePipelineState, Device, Function,
    FunctionConstantValues, Library, MTLDataType, MTLSize, NSUInteger,
};
use once_cell::sync::OnceCell;
use std::sync::{OnceLock, RwLock};
use std::{collections::HashMap, ffi::c_void};

pub mod utils;
use utils::EncoderProvider;

#[derive(Debug)]
pub enum PagedAttentionDType {
    F16 = 0,
    BF16 = 1,
    F32 = 2,
}

#[cfg(target_os = "macos")]
const KERNELS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/paged_attention.metallib"));
#[cfg(target_os = "ios")]
const KERNELS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/paged_attention_ios.metallib"));

#[derive(thiserror::Error, Debug)]
pub enum MetalKernelError {
    #[error("Could not lock kernel map: {0}")]
    LockError(String),
    #[error("Error while loading library: {0}")]
    LoadLibraryError(String),
    #[error("Error while loading function: {0:?}")]
    LoadFunctionError(String),
    #[error("Failed to create pipeline")]
    FailedToCreatePipeline(String),
    #[error("dtype mismatch, got {got:?}, expected {expected:?}")]
    DTypeMismatch { expected: Vec<DType>, got: DType },
}

impl<T> From<std::sync::PoisonError<T>> for MetalKernelError {
    fn from(e: std::sync::PoisonError<T>) -> Self {
        Self::LockError(e.to_string())
    }
}

type Pipelines = HashMap<(String, Option<ConstantValues>), ComputePipelineState>;

#[derive(Debug)]
pub struct Kernels {
    pipelines: RwLock<Pipelines>,
}

pub(crate) static G_KERNEL: OnceCell<Kernels> = OnceCell::new();
pub(crate) static LIBRARY: OnceLock<Library> = OnceLock::new();

impl Kernels {
    pub fn default() -> &'static Kernels {
        G_KERNEL.get_or_init(Kernels::new)
    }

    pub fn new() -> Self {
        let pipelines = RwLock::new(Pipelines::new());
        Self { pipelines }
    }

    pub fn load_library(&self, device: &Device) -> Result<Library, MetalKernelError> {
        if let Some(lib) = LIBRARY.get() {
            Ok(lib.clone())
        } else {
            let source_data = KERNELS;
            let lib = {
                device.new_library_with_data(source_data).map_err(|e| {
                    MetalKernelError::LoadLibraryError(format!(
                        "Metal requires macosx > 13.0 or higher, cannot load candle metal library: {e}"
                    ))
                })?
            };
            Ok(LIBRARY.get_or_init(|| lib).clone())
        }
    }

    fn load_function(
        &self,
        device: &Device,
        name: String,
        constants: Option<FunctionConstantValues>,
    ) -> Result<Function, MetalKernelError> {
        let func = self
            .load_library(device)?
            .get_function(&name, constants)
            .map_err(|e| MetalKernelError::LoadFunctionError(e.to_string()))?;
        Ok(func)
    }

    /// Load the give pipeline
    /// loads the library from source, then gets the function [`name`] from
    /// that source
    fn load_pipeline_with_constants(
        &self,
        device: &Device,
        name: String,
        constants: Option<ConstantValues>,
    ) -> Result<ComputePipelineState, MetalKernelError> {
        let mut pipelines = self.pipelines.write()?;
        let key = (name, constants);
        if let Some(pipeline) = pipelines.get(&key) {
            Ok(pipeline.clone())
        } else {
            let (name, constants) = key;
            let func = self.load_function(
                device,
                name.clone(),
                constants.as_ref().map(|c| c.function_constant_values()),
            )?;
            let pipeline = device
                .new_compute_pipeline_state_with_function(&func)
                .map_err(|e| MetalKernelError::FailedToCreatePipeline(e.to_string()))?;
            pipelines.insert((name, constants), pipeline.clone());

            Ok(pipeline)
        }
    }

    /// Load the give pipeline
    /// loads the library from source, then gets the function [`name`] from
    /// that source (without constants)
    pub fn load_pipeline(
        &self,
        device: &Device,
        name: String,
    ) -> Result<ComputePipelineState, MetalKernelError> {
        self.load_pipeline_with_constants(device, name, None)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn call_copy_blocks(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    key_cache: &Buffer,
    key_cache_offset: usize,
    value_cache: &Buffer,
    value_cache_offset: usize,
    block_mapping: &Buffer,
    block_mapping_offset: usize,
    num_pairs: u64,
    numel_per_block: u64,
) -> Result<(), MetalKernelError> {
    let name = match ty {
        DType::F32 => "copy_blocks_float",
        DType::BF16 => "copy_blocks_bfloat16_t",
        DType::F16 => "copy_blocks_half",
        other => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F32, DType::F16, DType::BF16],
                got: other,
            })
        }
    };
    let pipeline = kernels.load_pipeline(device, name.to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    set_params!(
        encoder,
        (
            (key_cache, key_cache_offset),
            (value_cache, value_cache_offset),
            (block_mapping, block_mapping_offset),
            numel_per_block
        )
    );

    let thread_groups_count = MTLSize {
        width: num_pairs,
        height: 1,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: numel_per_block.min(1024),
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_reshape_and_cache(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    key: &Buffer,
    key_offset: usize,
    value: &Buffer,
    value_offset: usize,
    key_cache: &Buffer,
    key_cache_offset: usize,
    value_cache: &Buffer,
    value_cache_offset: usize,
    slot_mapping: &Buffer,
    slot_mapping_offset: usize,
    num_tokens: i32,
    num_heads: i32,
    head_size: i32,
    block_size: i32,
    x: i32,
    key_stride: i32,
    value_stride: i32,
    k_scale: Option<MetalStorage>,
    v_scale: Option<MetalStorage>,
) -> Result<(), MetalKernelError> {
    let quantized_cache = k_scale.is_some() && v_scale.is_some();
    let name = match ty {
        PagedAttentionDType::F32 => {
            if quantized_cache {
                "reshape_and_cache_float_uint8_t"
            } else {
                "reshape_and_cache_float_float"
            }
        }
        PagedAttentionDType::BF16 => {
            if quantized_cache {
                "reshape_and_cache_bfloat16_t_uint8_t"
            } else {
                "reshape_and_cache_bfloat16_t_bfloat16_t"
            }
        }
        PagedAttentionDType::F16 => {
            if quantized_cache {
                "reshape_and_cache_half_uint8_t"
            } else {
                "reshape_and_cache_half_half"
            }
        }
    };
    let pipeline = kernels.load_pipeline(device, name.to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    set_params!(
        encoder,
        (
            (key, key_offset),
            (value, value_offset),
            (key_cache, key_cache_offset),
            (value_cache, value_cache_offset),
            (slot_mapping, slot_mapping_offset),
            key_stride,
            value_stride,
            num_heads,
            head_size,
            block_size,
            x,
            k_scale,
            v_scale
        )
    );

    let thread_groups_count = MTLSize {
        width: num_tokens as u64,
        height: 1,
        depth: 1,
    };
    let threads_per_threadgroup = MTLSize {
        width: (num_heads * head_size).min(512) as u64,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, threads_per_threadgroup);
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum Value {
    Bool(bool),
}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Value::Bool(v) => v.hash(state),
        }
    }
}

impl Value {
    fn data_type(&self) -> MTLDataType {
        match self {
            Value::Bool(_) => MTLDataType::Bool,
        }
    }
}

/// Not true, good enough for our purposes.
impl Eq for Value {}

#[derive(Debug, Eq, PartialEq, Hash)]
struct ConstantValues(Vec<(usize, Value)>);

impl ConstantValues {
    pub fn new(values: Vec<(usize, Value)>) -> Self {
        Self(values)
    }

    fn function_constant_values(&self) -> FunctionConstantValues {
        let f = FunctionConstantValues::new();
        for (index, value) in &self.0 {
            let ty = value.data_type();
            match value {
                Value::Bool(v) => {
                    f.set_constant_value_at_index(
                        v as *const bool as *const c_void,
                        ty,
                        *index as u64,
                    );
                }
            }
        }
        f
    }
}

#[allow(clippy::too_many_arguments)]
pub fn paged_attention_v1(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    q: &Buffer,
    q_offset: usize,
    k_cache: &Buffer,
    k_cache_offset: usize,
    v_cache: &Buffer,
    v_cache_offset: usize,
    block_tables: &Buffer,
    block_tables_offset: usize,
    context_lens: &Buffer,
    context_lens_offset: usize,
    alibi_storage_and_offset: Option<(MetalStorage, usize)>,
    output: &Buffer,
    num_kv_heads: i32,
    scale: f32,
    softcapping: f32,
    block_size: i32,
    max_context_len: i32,
    num_seqs: i32,
    num_heads: i32,
    head_size: i32,
    max_num_blocks_per_seq: i32,
    q_stride: i32,
    kv_block_stride: i32,
    kv_head_stride: i32,
    k_scale: Option<MetalStorage>,
    v_scale: Option<MetalStorage>,
) -> Result<(), MetalKernelError> {
    const NUM_THREADS: u64 = 256;
    const NUM_SIMD_LANES: u64 = 32;
    let quantized_cache = k_scale.is_some() && v_scale.is_some();

    let name = match ty {
        PagedAttentionDType::F32 => {
            if quantized_cache {
                "paged_attention_float_uint8_t"
            } else {
                "paged_attention_float_float"
            }
        }
        PagedAttentionDType::BF16 => {
            if quantized_cache {
                "paged_attention_bfloat16_t_uint8_t"
            } else {
                "paged_attention_bfloat16_t_bfloat16_t"
            }
        }
        PagedAttentionDType::F16 => {
            if quantized_cache {
                "paged_attention_half_uint8_t"
            } else {
                "paged_attention_half_half"
            }
        }
    };
    let mut name = name.to_string();
    name.push_str(&format!("_hs{head_size}"));
    name.push_str(&format!("_bs{block_size}"));
    name.push_str(&format!("_nt{NUM_THREADS}"));
    name.push_str(&format!("_nsl{}", NUM_SIMD_LANES));
    // v1 has no partition
    name.push_str(&format!("_ps{}", 0));

    // v1 has no partition.
    // Handle alibi
    let constants = Some(ConstantValues::new(vec![
        (10, Value::Bool(/* use_partitioning */ false)),
        (
            20,
            Value::Bool(/* use_alibi */ alibi_storage_and_offset.is_some()),
        ),
    ]));

    let pipeline = kernels.load_pipeline_with_constants(device, name, constants)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    assert_eq!(pipeline.thread_execution_width(), NUM_SIMD_LANES);

    let num_simds = NUM_THREADS / NUM_SIMD_LANES;
    let padded_max_context_len = ((max_context_len + block_size - 1) / block_size) * block_size;
    let logits_size = padded_max_context_len * std::mem::size_of::<f32>() as i32;
    let outputs_size = (num_simds as i32 / 2) * head_size * std::mem::size_of::<f32>() as i32;
    let shared_mem_size = logits_size.max(outputs_size);
    encoder.set_threadgroup_memory_length(0, shared_mem_size as u64);

    encoder.set_buffer(2, Some(output), 0 as NSUInteger);
    encoder.set_buffer(3, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(4, Some(k_cache), k_cache_offset as NSUInteger);
    encoder.set_buffer(5, Some(v_cache), v_cache_offset as NSUInteger);
    encoder.set_bytes(
        6,
        core::mem::size_of_val(&num_kv_heads) as u64,
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        7,
        core::mem::size_of_val(&scale) as u64,
        &scale as *const _ as *const c_void,
    );
    encoder.set_bytes(
        8,
        core::mem::size_of_val(&softcapping) as u64,
        &softcapping as *const _ as *const c_void,
    );
    encoder.set_buffer(9, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(10, Some(context_lens), context_lens_offset as NSUInteger);
    encoder.set_bytes(
        11,
        core::mem::size_of_val(&max_num_blocks_per_seq) as u64,
        &max_num_blocks_per_seq as *const _ as *const c_void,
    );
    if let Some((alibi, alibi_offset)) = alibi_storage_and_offset {
        encoder.set_buffer(12, Some(alibi.buffer()), alibi_offset as NSUInteger);
    }
    encoder.set_bytes(
        13,
        core::mem::size_of_val(&q_stride) as u64,
        &q_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        14,
        core::mem::size_of_val(&kv_block_stride) as u64,
        &kv_block_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        15,
        core::mem::size_of_val(&kv_head_stride) as u64,
        &kv_head_stride as *const _ as *const c_void,
    );

    if let Some(k_scale) = k_scale {
        encoder.set_buffer(16, Some(k_scale.buffer()), 0);
    }

    if let Some(v_scale) = v_scale {
        encoder.set_buffer(17, Some(v_scale.buffer()), 0);
    }

    let thread_groups_count = MTLSize {
        width: num_heads as u64,
        height: num_seqs as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: NUM_THREADS,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn paged_attention_v2(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    exp_sums: &Buffer,
    max_logits: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_cache: &Buffer,
    k_cache_offset: usize,
    v_cache: &Buffer,
    v_cache_offset: usize,
    block_tables: &Buffer,
    block_tables_offset: usize,
    context_lens: &Buffer,
    context_lens_offset: usize,
    alibi_storage_and_offset: Option<(MetalStorage, usize)>,
    tmp_out: &Buffer,
    output: &Buffer,
    num_kv_heads: i32,
    scale: f32,
    softcapping: f32,
    block_size: i32,
    max_context_len: i32,
    num_seqs: i32,
    num_heads: i32,
    head_size: i32,
    max_num_blocks_per_seq: i32,
    q_stride: i32,
    kv_block_stride: i32,
    kv_head_stride: i32,
    k_scale: Option<MetalStorage>,
    v_scale: Option<MetalStorage>,
) -> Result<(), MetalKernelError> {
    const NUM_THREADS: u64 = 256;
    const PARTITION_SIZE: u64 = 512;
    const NUM_SIMD_LANES: u64 = 32;
    let quantized_cache = k_scale.is_some() && v_scale.is_some();
    // Initial paged attention kernel
    {
        let name = match ty {
            PagedAttentionDType::F32 => {
                if quantized_cache {
                    "paged_attention_float_uint8_t"
                } else {
                    "paged_attention_float_float"
                }
            }
            PagedAttentionDType::BF16 => {
                if quantized_cache {
                    "paged_attention_bfloat16_t_uint8_t"
                } else {
                    "paged_attention_bfloat16_t_bfloat16_t"
                }
            }
            PagedAttentionDType::F16 => {
                if quantized_cache {
                    "paged_attention_half_uint8_t"
                } else {
                    "paged_attention_half_half"
                }
            }
        };
        let mut name = name.to_string();
        name.push_str(&format!("_hs{head_size}"));
        name.push_str(&format!("_bs{block_size}"));
        name.push_str(&format!("_nt{NUM_THREADS}"));
        name.push_str(&format!("_nsl{}", NUM_SIMD_LANES));
        // v2 has partition.
        name.push_str(&format!("_ps{}", PARTITION_SIZE));

        // v2 has partition.
        // Handle alibi
        let constants = Some(ConstantValues::new(vec![
            (10, Value::Bool(/* use_partitioning */ true)),
            (
                20,
                Value::Bool(/* use_alibi */ alibi_storage_and_offset.is_some()),
            ),
        ]));

        let pipeline = kernels.load_pipeline_with_constants(device, name, constants)?;

        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        assert_eq!(pipeline.thread_execution_width(), NUM_SIMD_LANES);

        let num_simds = NUM_THREADS / NUM_SIMD_LANES;
        let max_num_partitions =
            (max_context_len + PARTITION_SIZE as i32 - 1) / PARTITION_SIZE as i32;
        let logits_size = PARTITION_SIZE as i32 * std::mem::size_of::<f32>() as i32;
        let outputs_size = (num_simds as i32 / 2) * head_size * std::mem::size_of::<f32>() as i32;
        let shared_mem_size = logits_size.max(outputs_size);
        encoder.set_threadgroup_memory_length(0, shared_mem_size as u64);

        encoder.set_buffer(0, Some(exp_sums), 0 as NSUInteger);
        encoder.set_buffer(1, Some(max_logits), 0 as NSUInteger);
        encoder.set_buffer(2, Some(tmp_out), 0 as NSUInteger);
        encoder.set_buffer(3, Some(q), q_offset as NSUInteger);
        encoder.set_buffer(4, Some(k_cache), k_cache_offset as NSUInteger);
        encoder.set_buffer(5, Some(v_cache), v_cache_offset as NSUInteger);
        encoder.set_bytes(
            6,
            core::mem::size_of_val(&num_kv_heads) as u64,
            &num_kv_heads as *const _ as *const c_void,
        );
        encoder.set_bytes(
            7,
            core::mem::size_of_val(&scale) as u64,
            &scale as *const _ as *const c_void,
        );
        encoder.set_bytes(
            8,
            core::mem::size_of_val(&softcapping) as u64,
            &softcapping as *const _ as *const c_void,
        );
        encoder.set_buffer(9, Some(block_tables), block_tables_offset as NSUInteger);
        encoder.set_buffer(10, Some(context_lens), context_lens_offset as NSUInteger);
        encoder.set_bytes(
            11,
            core::mem::size_of_val(&max_num_blocks_per_seq) as u64,
            &max_num_blocks_per_seq as *const _ as *const c_void,
        );
        if let Some((alibi, alibi_offset)) = alibi_storage_and_offset {
            encoder.set_buffer(12, Some(alibi.buffer()), alibi_offset as NSUInteger);
        }
        encoder.set_bytes(
            13,
            core::mem::size_of_val(&q_stride) as u64,
            &q_stride as *const _ as *const c_void,
        );
        encoder.set_bytes(
            14,
            core::mem::size_of_val(&kv_block_stride) as u64,
            &kv_block_stride as *const _ as *const c_void,
        );
        encoder.set_bytes(
            15,
            core::mem::size_of_val(&kv_head_stride) as u64,
            &kv_head_stride as *const _ as *const c_void,
        );

        if let Some(k_scale) = k_scale {
            encoder.set_buffer(16, Some(k_scale.buffer()), 0);
        }

        if let Some(v_scale) = v_scale {
            encoder.set_buffer(17, Some(v_scale.buffer()), 0);
        }

        let thread_groups_count = MTLSize {
            width: num_heads as u64,
            height: num_seqs as u64,
            depth: max_num_partitions as u64,
        };
        let thread_group_size = MTLSize {
            width: NUM_THREADS,
            height: 1,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }

    // Paged attention reduce kernel
    {
        let name = match ty {
            PagedAttentionDType::F32 => {
                if quantized_cache {
                    "paged_attention_v2_reduce_float_uint8_t"
                } else {
                    "paged_attention_v2_reduce_float_float"
                }
            }
            PagedAttentionDType::BF16 => {
                if quantized_cache {
                    "paged_attention_v2_reduce_bfloat16_t_uint8_t"
                } else {
                    "paged_attention_v2_reduce_bfloat16_t_bfloat16_t"
                }
            }
            PagedAttentionDType::F16 => {
                if quantized_cache {
                    "paged_attention_v2_reduce_half_uint8_t"
                } else {
                    "paged_attention_v2_reduce_half_half"
                }
            }
        };
        let mut name = name.to_string();
        name.push_str(&format!("_hs{head_size}"));
        name.push_str(&format!("_nt{NUM_THREADS}"));
        name.push_str(&format!("_nsl{}", NUM_SIMD_LANES));
        name.push_str(&format!("_ps{}", PARTITION_SIZE));

        let pipeline = kernels.load_pipeline(device, name)?;

        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        assert_eq!(pipeline.thread_execution_width(), NUM_SIMD_LANES);

        let max_num_partitions =
            (max_context_len + PARTITION_SIZE as i32 - 1) / PARTITION_SIZE as i32;
        let reduce_shared_mem_size = 2 * max_num_partitions * std::mem::size_of::<f32>() as i32;
        encoder.set_threadgroup_memory_length(0, reduce_shared_mem_size as u64);

        encoder.set_buffer(0, Some(output), 0 as NSUInteger);
        encoder.set_buffer(1, Some(exp_sums), 0 as NSUInteger);
        encoder.set_buffer(2, Some(max_logits), 0 as NSUInteger);
        encoder.set_buffer(3, Some(tmp_out), 0 as NSUInteger);
        encoder.set_buffer(4, Some(context_lens), context_lens_offset as NSUInteger);
        encoder.set_bytes(
            5,
            core::mem::size_of_val(&max_num_partitions) as u64,
            &max_num_partitions as *const _ as *const c_void,
        );

        let thread_groups_count = MTLSize {
            width: num_heads as u64,
            height: num_seqs as u64,
            depth: 1,
        };
        let thread_group_size = MTLSize {
            width: NUM_THREADS,
            height: 1,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }
    Ok(())
}

/// Launches the `chunked_prefill_paged_attention` Metal kernel.
///
/// This kernel is optimized for the prefill (prompt processing) stage of attention, where a batch of new tokens is processed.
/// The strategy is to assign one thread to each token in the query sequence. The dispatch grid is structured to parallelize work across
/// query heads, key-value heads, and chunks of tokens.
///
/// # Dispatch Logic
/// - **Threadgroup Size:** `(TOKEN_CHUNK_SIZE, 1, 1)`. Each threadgroup processes a chunk of `TOKEN_CHUNK_SIZE` tokens.
/// - **Grid Dimensions:** `(num_queries_per_kv, num_kv_heads, num_token_chunks)`.
///   - `width`: Parallelizes over the query heads that map to a single key-value head.
///   - `height`: Parallelizes over the key-value heads.
///   - `depth`: Parallelizes over the chunks of query tokens.
///
/// # Arguments
///
/// * `device` - The Metal device to execute the kernel on.
/// * `ep` - An `EncoderProvider` to get a command encoder.
/// * `kernels` - A struct for loading and caching Metal pipeline states.
/// * `ty` - The data type (`F16`, `BF16`, `F32`) of the tensors.
/// * `output` - The output buffer for the attention results. Shape: `[num_query_tokens, num_query_heads, head_size]`.
/// * `q` - The query tensor.
/// * `k_cache` - The paged key-cache.
/// * `v_cache` - The paged value-cache.
/// * `block_tables` - A tensor mapping logical sequence blocks to physical blocks in the cache. Shape: `[num_seqs, max_num_blocks_per_seq]`.
/// * `seq_lens` - A buffer containing the full context length of each sequence.
/// * `query_start_len` - A buffer indicating the start token index for each sequence in the flattened query tensor.
/// * `alibi_slopes` - Optional buffer containing ALiBi slopes for positional bias.
/// * `k_scale` - Optional buffer for per-head FP8 K scales.
/// * `v_scale` - Optional buffer for per-head FP8 V scales.
/// * `sinks` - Optional buffer for sink attention.
/// * `num_kv_heads` - The number of key-value heads (for Grouped-Query Attention).
/// * `scale` - The softmax scaling factor (typically `1.0 / sqrt(head_size)`).
/// * `block_table_stride` - The stride of the `block_tables` tensor (i.e., `max_num_blocks_per_seq`).
/// * `num_seqs` - The number of sequences in the batch.
/// * `num_query_heads` - The total number of query heads.
/// * `num_query_tokens` - The total number of tokens being processed in this prefill run.
/// * `head_size` - The dimension of each attention head.
/// * `block_size` - The number of tokens per block in the KV cache.
/// * `softcapping` - Softcapping value for the tanh activation on attention scores.
/// * `o_stride_tokens` - The stride of the output tensor's first dimension.
/// * `sliding_window` - The sliding window size for attention, if applicable.
/// * `total_num_blocks` - The total number of physical blocks in the KV cache.
/// * `kv_block_stride` - The stride between blocks in the KV cache.
/// * `kv_head_stride` - The stride between heads in the KV cache.
#[allow(clippy::too_many_arguments)]
pub fn paged_attention_prefill(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    // Buffers and Offsets
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_cache: &Buffer,
    k_cache_offset: usize,
    v_cache: &Buffer,
    v_cache_offset: usize,
    block_tables: &Buffer,
    block_tables_offset: usize,
    seq_lens: &Buffer, // Equivalent to `context_lens` in the v1 kernel
    seq_lens_offset: usize,
    query_start_len: &Buffer,
    query_start_len_offset: usize,
    alibi_slopes: Option<(MetalStorage, usize)>,
    k_scale: Option<MetalStorage>,
    v_scale: Option<MetalStorage>,
    sinks: Option<(MetalStorage, usize)>,
    // Scalar Parameters
    num_kv_heads: i32,
    scale: f32,              // sm_scale
    block_table_stride: i32, // max_num_blocks_per_seq
    num_seqs: i32,
    num_query_heads: i32,
    num_query_tokens: i32,
    head_size: i32,
    block_size: i32,
    softcapping: f32,
    o_stride_tokens: i32,
    sliding_window: i32,
    total_num_blocks: i32,
    kv_block_stride: i32,
    kv_head_stride: i32,
) -> Result<(), MetalKernelError> {
    // This value must match the `token_chunk_size` used in the .metal instantiation macros
    const TOKEN_CHUNK_SIZE: u64 = 64;
    let quantized_cache = k_scale.is_some() && v_scale.is_some();

    // 1. Construct the unique kernel name from its template parameters.
    let type_name = match ty {
        PagedAttentionDType::F32 => "float",
        PagedAttentionDType::BF16 => "bfloat16_t",
        PagedAttentionDType::F16 => "half",
    };
    let name = if quantized_cache {
        format!(
            "chunked_prefill_{}_uint8_t_hs{}_bs{}_tcs{}",
            type_name, head_size, block_size, TOKEN_CHUNK_SIZE
        )
    } else {
        format!(
            "chunked_prefill_{}_hs{}_bs{}_tcs{}",
            type_name, head_size, block_size, TOKEN_CHUNK_SIZE
        )
    };

    // 2. Load the pipeline. The prefill kernel does not use function constants.
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    // NOTE: Unlike the v1 kernel, the chunked prefill kernel is designed to use
    // registers and local arrays instead of threadgroup memory, so we do not
    // call `set_threadgroup_memory_length`.

    // 3. Set all kernel arguments, matching the `[[buffer(n)]]` indices.
    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_cache), k_cache_offset as NSUInteger);
    encoder.set_buffer(3, Some(v_cache), v_cache_offset as NSUInteger);
    encoder.set_bytes(
        4,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(5, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_buffer(6, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(7, Some(seq_lens), seq_lens_offset as NSUInteger);
    encoder.set_bytes(
        8,
        size_of_val(&block_table_stride),
        &block_table_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        9,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        10,
        size_of_val(&num_query_heads),
        &num_query_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        11,
        size_of_val(&num_query_tokens),
        &num_query_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        12,
        size_of_val(&softcapping),
        &softcapping as *const _ as *const c_void,
    );
    encoder.set_bytes(
        13,
        size_of_val(&o_stride_tokens),
        &o_stride_tokens as *const _ as *const c_void,
    );
    encoder.set_buffer(
        14,
        Some(query_start_len),
        query_start_len_offset as NSUInteger,
    );
    if let Some((slop, offset)) = alibi_slopes {
        encoder.set_buffer(15, Some(slop.buffer()), offset as NSUInteger);
    }
    if let Some(k_scale) = k_scale {
        encoder.set_buffer(16, Some(k_scale.buffer()), 0);
    }
    if let Some(v_scale) = v_scale {
        encoder.set_buffer(17, Some(v_scale.buffer()), 0);
    }
    if let Some((sk, offset)) = sinks {
        encoder.set_buffer(18, Some(sk.buffer()), offset as NSUInteger);
    }
    encoder.set_bytes(
        19,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );
    encoder.set_bytes(
        20,
        size_of_val(&total_num_blocks),
        &total_num_blocks as *const _ as *const c_void,
    );
    encoder.set_bytes(
        21,
        size_of_val(&kv_block_stride),
        &kv_block_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        22,
        size_of_val(&kv_head_stride),
        &kv_head_stride as *const _ as *const c_void,
    );

    // 4. Calculate grid and threadgroup dimensions, matching the CUDA launch config.
    // CUDA: dim3 block(TOKEN_CHUNK_SIZE);
    let thread_group_size = MTLSize {
        width: TOKEN_CHUNK_SIZE,
        height: 1,
        depth: 1,
    };

    // CUDA: dim3 grid(num_queries_per_kv, num_kv_heads, num_token_chunks);
    let num_queries_per_kv = (num_query_heads / num_kv_heads) as u64;
    let num_token_chunks = (num_query_tokens as u64 + TOKEN_CHUNK_SIZE - 1) / TOKEN_CHUNK_SIZE;
    let thread_groups_count = MTLSize {
        width: num_queries_per_kv,
        height: num_kv_heads as u64,
        depth: num_token_chunks,
    };

    // 5. Dispatch the kernel.
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

// Helper function to get size of a value for set_bytes
fn size_of_val<T>(val: &T) -> u64 {
    core::mem::size_of_val(val) as u64
}

#[allow(clippy::too_many_arguments)]
pub fn call_causal_mask(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    out: &Buffer,
    tgt_len: i32,
    sliding_window: i32,
) -> Result<(), MetalKernelError> {
    // Determine the kernel name based on the type
    let name = match ty {
        PagedAttentionDType::F32 => "causal_mask_float",
        PagedAttentionDType::BF16 => "causal_mask_bfloat16_t",
        PagedAttentionDType::F16 => "causal_mask_half",
    };

    // Load the compute pipeline for the selected type
    let pipeline = kernels.load_pipeline(device, name.to_string())?;

    // Get the encoder and cast it to the appropriate type
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    // Set the parameters for the kernel
    set_params!(
        encoder,
        (
            (out, 0),       // Output buffer
            tgt_len,        // Target length (size)
            sliding_window  // Sliding window size
        )
    );

    // Set up the number of thread groups and threads per group
    let thread_groups_count = MTLSize {
        width: tgt_len as u64,
        height: 1,
        depth: 1,
    };
    let threads_per_threadgroup = MTLSize {
        width: 256, // Use a reasonable number of threads per threadgroup
        height: 1,
        depth: 1,
    };

    // Dispatch the kernel
    encoder.dispatch_thread_groups(thread_groups_count, threads_per_threadgroup);

    // Commit the encoder to launch the kernel
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn call_update_scales_per_head(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    k: &Buffer,
    v: &Buffer,
    num_tokens: i64,
    num_heads: i32,
    head_dim: i32,
    k_scales: &Buffer,
    v_scales: &Buffer,
) -> Result<(), MetalKernelError> {
    let name = match ty {
        PagedAttentionDType::F32 => "compute_and_update_scales_per_head_float",
        PagedAttentionDType::BF16 => "compute_and_update_scales_per_head_bfloat16_t",
        PagedAttentionDType::F16 => "compute_and_update_scales_per_head_half",
    };

    let pipeline = kernels.load_pipeline(device, name.to_string())?;

    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    set_params!(
        encoder,
        (
            (k, 0),
            (v, 0),
            num_tokens,
            num_heads,
            head_dim,
            (k_scales, 0),
            (v_scales, 0)
        )
    );

    let threads_per_threadgroup = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };

    encoder.dispatch_thread_groups(thread_groups_count, threads_per_threadgroup);
    Ok(())
}

/// Fused Rotary Position Embedding with position selection
///
/// This kernel fuses index_select + RoPE into a single kernel.
/// Supports GQA (different head counts for Q and K).
///
/// # Arguments
/// * `q` - Query buffer [batch, num_q_heads, seq_len, head_dim]
/// * `k` - Key buffer [batch, num_kv_heads, seq_len, head_dim]
/// * `cos` - Full cosine table [max_seq_len, head_dim/2]
/// * `sin` - Full sine table [max_seq_len, head_dim/2]
/// * `positions` - Position indices [seq_len] (i64)
/// * `q_bh` - batch * num_q_heads
/// * `k_bh` - batch * num_kv_heads
/// * `seq_len` - sequence length
/// * `d` - head_dim
/// * `rotary_dim` - number of channels to rotate
/// * `is_interleaved` - if true, use interleaved RoPE layout
/// * `is_token_major` - if true, Q/K layout is [tokens, heads, dim]
#[allow(clippy::too_many_arguments)]
pub fn call_fused_rope(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    q: &Buffer,
    q_offset: usize,
    k: &Buffer,
    k_offset: usize,
    cos: &Buffer,
    cos_offset: usize,
    sin: &Buffer,
    sin_offset: usize,
    positions: &Buffer,
    positions_offset: usize,
    q_bh: u32,
    k_bh: u32,
    seq_len: u32,
    d: u32,
    rotary_dim: u32,
    is_interleaved: bool,
    is_token_major: bool,
) -> Result<(), MetalKernelError> {
    let type_name = match ty {
        DType::F32 => "f32",
        DType::BF16 => "bf16",
        DType::F16 => "f16",
        other => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F32, DType::F16, DType::BF16],
                got: other,
            })
        }
    };

    let name = if is_interleaved {
        format!("fused_rope_i_{}", type_name)
    } else {
        format!("fused_rope_{}", type_name)
    };

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    set_params!(
        encoder,
        (
            (q, q_offset),
            (k, k_offset),
            (cos, cos_offset),
            (sin, sin_offset),
            (positions, positions_offset),
            q_bh,
            k_bh,
            seq_len,
            d,
            rotary_dim,
            if is_token_major { 1u32 } else { 0u32 }
        )
    );

    // Calculate total number of pairs
    let rotary_pairs = rotary_dim / 2;
    let total_pairs = ((q_bh + k_bh) * seq_len * rotary_pairs) as u64;

    // Dispatch with 256 threads per threadgroup
    let threads_per_threadgroup = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: (total_pairs + 255) / 256,
        height: 1,
        depth: 1,
    };

    encoder.dispatch_thread_groups(thread_groups_count, threads_per_threadgroup);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_fp8_matmul(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    input: &Buffer,
    input_offset: usize,
    weight: &Buffer,
    weight_offset: usize,
    weight_scale: &Buffer,
    weight_scale_offset: usize,
    output: &Buffer,
    output_offset: usize,
    m: i32,
    n: i32,
    k: i32,
    scale_row_stride: i32,
    block_size_y: i32,
    block_size_x: i32,
) -> Result<(), MetalKernelError> {
    let is_gemv = m <= 8;
    let name = match ty {
        DType::F16 => {
            if is_gemv {
                "fp8_gemv_half"
            } else if m <= 16 {
                "fp8_matmul_half_16_32_32"
            } else {
                "fp8_matmul_half_32_32_32"
            }
        }
        DType::BF16 => {
            if is_gemv {
                "fp8_gemv_bfloat16"
            } else if m <= 16 {
                "fp8_matmul_bfloat16_16_32_32"
            } else {
                "fp8_matmul_bfloat16_32_32_32"
            }
        }
        _ => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: ty,
            })
        }
    };
    let pipeline = kernels.load_pipeline(device, name.to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    set_params!(
        encoder,
        (
            (input, input_offset),
            (weight, weight_offset),
            (weight_scale, weight_scale_offset),
            (output, output_offset),
            m,
            n,
            k,
            scale_row_stride,
            block_size_y,
            block_size_x
        )
    );

    if is_gemv {
        // Eight SIMD groups share each activation tile.
        let thread_group_size = MTLSize {
            width: 256,
            height: 1,
            depth: 1,
        };
        let thread_groups_count = MTLSize {
            width: (n as u64 + 7) / 8,
            height: m as u64,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    } else {
        let thread_group_size = MTLSize {
            width: 32,
            height: 1,
            depth: 1,
        };
        let m_block = if m <= 16 { 16 } else { 32 };
        let thread_groups_count = MTLSize {
            width: (n as u64 + 31) / 32,
            height: (m as u64 + m_block - 1) / m_block,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// MoE GEMM
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_moe_gemm(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    input: &Buffer,
    input_offset: usize,
    weights: &Buffer,
    weights_offset: usize,
    sorted_token_ids: &Buffer,
    sorted_token_ids_offset: usize,
    expert_ids: &Buffer,
    expert_ids_offset: usize,
    topk_weights: Option<(&Buffer, usize)>,
    output: &Buffer,
    output_offset: usize,
    num_experts: i32,
    topk: i32,
    size_m: i32,
    size_n: i32,
    size_k: i32,
    is_prefill: bool,
) -> Result<(), MetalKernelError> {
    let use_gemv = !is_prefill && size_m <= 128;
    let has_topk_weights: i32 = if topk_weights.is_some() { 1 } else { 0 };

    // Dummy buffer for nullable topk_weights
    let dummy_offset: usize = 0;

    if use_gemv {
        let name = match ty {
            DType::F16 => "moe_gemv_half",
            DType::BF16 => "moe_gemv_bfloat16",
            other => {
                return Err(MetalKernelError::DTypeMismatch {
                    expected: vec![DType::F16, DType::BF16],
                    got: other,
                })
            }
        };
        let pipeline = kernels.load_pipeline(device, name.to_string())?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        encoder.set_buffer(0, Some(input), input_offset as NSUInteger);
        encoder.set_buffer(1, Some(weights), weights_offset as NSUInteger);
        encoder.set_buffer(
            2,
            Some(sorted_token_ids),
            sorted_token_ids_offset as NSUInteger,
        );
        encoder.set_buffer(3, Some(expert_ids), expert_ids_offset as NSUInteger);
        if let Some((tw_buf, tw_off)) = topk_weights {
            encoder.set_buffer(4, Some(tw_buf), tw_off as NSUInteger);
        } else {
            encoder.set_buffer(4, Some(output), dummy_offset as NSUInteger);
        }
        encoder.set_buffer(5, Some(output), output_offset as NSUInteger);
        utils::set_param(encoder, 6, num_experts);
        utils::set_param(encoder, 7, topk);
        utils::set_param(encoder, 8, size_m);
        utils::set_param(encoder, 9, size_n);
        utils::set_param(encoder, 10, size_k);
        utils::set_param(encoder, 11, has_topk_weights);

        let thread_group_size = MTLSize {
            width: 32,
            height: 1,
            depth: 1,
        };
        let thread_groups_count = MTLSize {
            width: size_n as u64,
            height: size_m as u64,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    } else {
        const BLOCK_M: u64 = 32;
        const BLOCK_N: u64 = 32;

        let name = match ty {
            DType::F16 => "moe_gemm_half_32_32_32",
            DType::BF16 => "moe_gemm_bfloat16_32_32_32",
            other => {
                return Err(MetalKernelError::DTypeMismatch {
                    expected: vec![DType::F16, DType::BF16],
                    got: other,
                })
            }
        };
        let pipeline = kernels.load_pipeline(device, name.to_string())?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        encoder.set_buffer(0, Some(input), input_offset as NSUInteger);
        encoder.set_buffer(1, Some(weights), weights_offset as NSUInteger);
        encoder.set_buffer(
            2,
            Some(sorted_token_ids),
            sorted_token_ids_offset as NSUInteger,
        );
        encoder.set_buffer(3, Some(expert_ids), expert_ids_offset as NSUInteger);
        if let Some((tw_buf, tw_off)) = topk_weights {
            encoder.set_buffer(4, Some(tw_buf), tw_off as NSUInteger);
        } else {
            encoder.set_buffer(4, Some(output), dummy_offset as NSUInteger);
        }
        encoder.set_buffer(5, Some(output), output_offset as NSUInteger);
        utils::set_param(encoder, 6, num_experts);
        utils::set_param(encoder, 7, topk);
        utils::set_param(encoder, 8, size_m);
        utils::set_param(encoder, 9, size_n);
        utils::set_param(encoder, 10, size_k);
        utils::set_param(encoder, 11, has_topk_weights);

        let thread_group_size = MTLSize {
            width: 32,
            height: 1,
            depth: 1,
        };
        let thread_groups_count = MTLSize {
            width: (size_n as u64 + BLOCK_N - 1) / BLOCK_N,
            height: (size_m as u64 + BLOCK_M - 1) / BLOCK_M,
            depth: num_experts as u64,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// MoE GGUF GEMM/GEMV — quantized weights, on-the-fly dequantization
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_moe_gguf_gemm(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    input: &Buffer,
    input_offset: usize,
    weights: &Buffer,
    weights_offset: usize,
    sorted_token_ids: &Buffer,
    sorted_token_ids_offset: usize,
    expert_ids: &Buffer,
    expert_ids_offset: usize,
    topk_weights: Option<(&Buffer, usize)>,
    output: &Buffer,
    output_offset: usize,
    num_experts: i32,
    topk: i32,
    size_m: i32,
    size_n: i32,
    size_k: i32,
    is_prefill: bool,
    gguf_type: i32,
    block_size_bytes: i32,
    qk: i32,
) -> Result<(), MetalKernelError> {
    // Always take the per-row GEMV path. The tiled GEMM below dequantises one
    // "reference expert" per 32-row tile and applies it to every row in the
    // tile, which is only right when all 32 sorted rows share an expert; with
    // 256 experts and a few hundred rows that is essentially never true, and
    // prefill produced garbage. GEMV handles one (row, expert) pair per
    // simdgroup and matches a CPU reference to ~1e-3. Slower for very long
    // prompts, but correct.
    let _ = is_prefill;
    let use_gemv = true;
    let has_topk_weights: i32 = if topk_weights.is_some() { 1 } else { 0 };
    let dummy_offset: usize = 0;

    if use_gemv {
        let name = match ty {
            DType::F16 => "moe_gguf_gemv_half",
            DType::BF16 => "moe_gguf_gemv_bfloat16",
            other => {
                return Err(MetalKernelError::DTypeMismatch {
                    expected: vec![DType::F16, DType::BF16],
                    got: other,
                })
            }
        };
        let pipeline = kernels.load_pipeline(device, name.to_string())?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        encoder.set_buffer(0, Some(input), input_offset as NSUInteger);
        encoder.set_buffer(1, Some(weights), weights_offset as NSUInteger);
        encoder.set_buffer(
            2,
            Some(sorted_token_ids),
            sorted_token_ids_offset as NSUInteger,
        );
        encoder.set_buffer(3, Some(expert_ids), expert_ids_offset as NSUInteger);
        if let Some((tw_buf, tw_off)) = topk_weights {
            encoder.set_buffer(4, Some(tw_buf), tw_off as NSUInteger);
        } else {
            encoder.set_buffer(4, Some(output), dummy_offset as NSUInteger);
        }
        encoder.set_buffer(5, Some(output), output_offset as NSUInteger);
        utils::set_param(encoder, 6, num_experts);
        utils::set_param(encoder, 7, topk);
        utils::set_param(encoder, 8, size_m);
        utils::set_param(encoder, 9, size_n);
        utils::set_param(encoder, 10, size_k);
        utils::set_param(encoder, 11, has_topk_weights);
        utils::set_param(encoder, 12, gguf_type);
        utils::set_param(encoder, 13, block_size_bytes);
        utils::set_param(encoder, 14, qk);

        let thread_group_size = MTLSize {
            width: 32,
            height: 4,
            depth: 1,
        };
        let thread_groups_count = MTLSize {
            width: ((size_n as u64) + 3) / 4,
            height: size_m as u64,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    } else {
        const BLOCK_M: u64 = 32;
        const BLOCK_N: u64 = 32;

        let name = match ty {
            DType::F16 => "moe_gguf_gemm_half",
            DType::BF16 => "moe_gguf_gemm_bfloat16",
            other => {
                return Err(MetalKernelError::DTypeMismatch {
                    expected: vec![DType::F16, DType::BF16],
                    got: other,
                })
            }
        };
        let pipeline = kernels.load_pipeline(device, name.to_string())?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        encoder.set_buffer(0, Some(input), input_offset as NSUInteger);
        encoder.set_buffer(1, Some(weights), weights_offset as NSUInteger);
        encoder.set_buffer(
            2,
            Some(sorted_token_ids),
            sorted_token_ids_offset as NSUInteger,
        );
        encoder.set_buffer(3, Some(expert_ids), expert_ids_offset as NSUInteger);
        if let Some((tw_buf, tw_off)) = topk_weights {
            encoder.set_buffer(4, Some(tw_buf), tw_off as NSUInteger);
        } else {
            encoder.set_buffer(4, Some(output), dummy_offset as NSUInteger);
        }
        encoder.set_buffer(5, Some(output), output_offset as NSUInteger);
        utils::set_param(encoder, 6, num_experts);
        utils::set_param(encoder, 7, topk);
        utils::set_param(encoder, 8, size_m);
        utils::set_param(encoder, 9, size_n);
        utils::set_param(encoder, 10, size_k);
        utils::set_param(encoder, 11, has_topk_weights);
        utils::set_param(encoder, 12, gguf_type);
        utils::set_param(encoder, 13, block_size_bytes);
        utils::set_param(encoder, 14, qk);

        let thread_group_size = MTLSize {
            width: 32,
            height: 1,
            depth: 1,
        };
        let thread_groups_count = MTLSize {
            width: (size_n as u64 + BLOCK_N - 1) / BLOCK_N,
            height: (size_m as u64 + BLOCK_M - 1) / BLOCK_M,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// NVFP4 MoE GEMM
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_nvfp4_moe_gemm(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    x: (&Buffer, usize),
    w: (&Buffer, usize),
    scales: (&Buffer, usize),
    global_scales: (&Buffer, usize),
    indices: (&Buffer, usize),
    topk_weights: Option<(&Buffer, usize)>,
    out: &Buffer,
    num_tokens: usize,
    topk: usize,
    num_experts: usize,
    n: usize,
    k: usize,
    input_has_topk_dim: bool,
    reuse_topk: bool,
) -> Result<(), MetalKernelError> {
    let name = match (reuse_topk, ty) {
        (true, DType::F16) => "nvfp4_moe_gemm_reuse_f16",
        (true, DType::BF16) => "nvfp4_moe_gemm_reuse_bf16",
        (false, DType::F16) => "nvfp4_moe_gemm_split_f16",
        (false, DType::BF16) => "nvfp4_moe_gemm_split_bf16",
        (_, other) => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: other,
            })
        }
    };

    let has_topk_weights: i32 = if topk_weights.is_some() { 1 } else { 0 };

    let pipeline = kernels.load_pipeline(device, name.to_string())?;

    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(x.0), x.1 as NSUInteger);
    encoder.set_buffer(1, Some(w.0), w.1 as NSUInteger);
    encoder.set_buffer(2, Some(scales.0), scales.1 as NSUInteger);
    encoder.set_buffer(3, Some(global_scales.0), global_scales.1 as NSUInteger);
    encoder.set_buffer(4, Some(indices.0), indices.1 as NSUInteger);
    if let Some((tw_buf, tw_off)) = topk_weights {
        encoder.set_buffer(5, Some(tw_buf), tw_off as NSUInteger);
    } else {
        encoder.set_buffer(5, Some(out), 0 as NSUInteger);
    }
    encoder.set_buffer(6, Some(out), 0 as NSUInteger);

    if reuse_topk {
        utils::set_param(encoder, 7, num_tokens as i32);
        utils::set_param(encoder, 8, topk as i32);
        utils::set_param(encoder, 9, num_experts as i32);
        utils::set_param(encoder, 10, n as i32);
        utils::set_param(encoder, 11, k as i32);
        utils::set_param(encoder, 12, has_topk_weights);
    } else {
        utils::set_param(encoder, 7, num_tokens as i32);
        utils::set_param(encoder, 8, topk as i32);
        utils::set_param(encoder, 9, num_experts as i32);
        utils::set_param(encoder, 10, n as i32);
        utils::set_param(encoder, 11, k as i32);
        utils::set_param(encoder, 12, input_has_topk_dim as i32);
        utils::set_param(encoder, 13, has_topk_weights);
    }

    let group_dims = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let grid_dims = MTLSize {
        width: ((n + 7) / 8) as u64,
        height: num_tokens as u64,
        depth: if reuse_topk { 1 } else { topk as u64 },
    };
    encoder.dispatch_thread_groups(grid_dims, group_dims);
    Ok(())
}

// ---------------------------------------------------------------------------
// FP8 MoE GEMM
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_fp8_moe_gemm(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    input: &Buffer,
    input_offset: usize,
    weights: &Buffer,
    weights_offset: usize,
    weight_scales: &Buffer,
    weight_scales_offset: usize,
    sorted_token_ids: &Buffer,
    sorted_token_ids_offset: usize,
    expert_ids: &Buffer,
    expert_ids_offset: usize,
    topk_weights: Option<(&Buffer, usize)>,
    output: &Buffer,
    output_offset: usize,
    num_experts: i32,
    topk: i32,
    size_m: i32,
    size_n: i32,
    size_k: i32,
    block_size_n: i32,
    block_size_k: i32,
    is_prefill: bool,
) -> Result<(), MetalKernelError> {
    let use_gemv = !is_prefill && size_m <= 128;
    let has_topk_weights: i32 = if topk_weights.is_some() { 1 } else { 0 };

    let dummy_offset: usize = 0;

    if use_gemv {
        let name = match ty {
            DType::F16 => "fp8_moe_gemv_half",
            DType::BF16 => "fp8_moe_gemv_bfloat16",
            other => {
                return Err(MetalKernelError::DTypeMismatch {
                    expected: vec![DType::F16, DType::BF16],
                    got: other,
                })
            }
        };
        let pipeline = kernels.load_pipeline(device, name.to_string())?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        encoder.set_buffer(0, Some(input), input_offset as NSUInteger);
        encoder.set_buffer(1, Some(weights), weights_offset as NSUInteger);
        encoder.set_buffer(2, Some(weight_scales), weight_scales_offset as NSUInteger);
        encoder.set_buffer(
            3,
            Some(sorted_token_ids),
            sorted_token_ids_offset as NSUInteger,
        );
        encoder.set_buffer(4, Some(expert_ids), expert_ids_offset as NSUInteger);
        if let Some((tw_buf, tw_off)) = topk_weights {
            encoder.set_buffer(5, Some(tw_buf), tw_off as NSUInteger);
        } else {
            encoder.set_buffer(5, Some(output), dummy_offset as NSUInteger);
        }
        encoder.set_buffer(6, Some(output), output_offset as NSUInteger);
        utils::set_param(encoder, 7, num_experts);
        utils::set_param(encoder, 8, topk);
        utils::set_param(encoder, 9, size_m);
        utils::set_param(encoder, 10, size_n);
        utils::set_param(encoder, 11, size_k);
        utils::set_param(encoder, 12, has_topk_weights);
        utils::set_param(encoder, 13, block_size_n);
        utils::set_param(encoder, 14, block_size_k);

        let thread_group_size = MTLSize {
            width: 32,
            height: 1,
            depth: 1,
        };
        let thread_groups_count = MTLSize {
            width: size_n as u64,
            height: size_m as u64,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    } else {
        const BLOCK_M: u64 = 32;
        const BLOCK_N: u64 = 32;

        let name = match ty {
            DType::F16 => "fp8_moe_gemm_half_32_32",
            DType::BF16 => "fp8_moe_gemm_bfloat16_32_32",
            other => {
                return Err(MetalKernelError::DTypeMismatch {
                    expected: vec![DType::F16, DType::BF16],
                    got: other,
                })
            }
        };
        let pipeline = kernels.load_pipeline(device, name.to_string())?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        encoder.set_buffer(0, Some(input), input_offset as NSUInteger);
        encoder.set_buffer(1, Some(weights), weights_offset as NSUInteger);
        encoder.set_buffer(2, Some(weight_scales), weight_scales_offset as NSUInteger);
        encoder.set_buffer(
            3,
            Some(sorted_token_ids),
            sorted_token_ids_offset as NSUInteger,
        );
        encoder.set_buffer(4, Some(expert_ids), expert_ids_offset as NSUInteger);
        if let Some((tw_buf, tw_off)) = topk_weights {
            encoder.set_buffer(5, Some(tw_buf), tw_off as NSUInteger);
        } else {
            encoder.set_buffer(5, Some(output), dummy_offset as NSUInteger);
        }
        encoder.set_buffer(6, Some(output), output_offset as NSUInteger);
        utils::set_param(encoder, 7, num_experts);
        utils::set_param(encoder, 8, topk);
        utils::set_param(encoder, 9, size_m);
        utils::set_param(encoder, 10, size_n);
        utils::set_param(encoder, 11, size_k);
        utils::set_param(encoder, 12, has_topk_weights);
        utils::set_param(encoder, 13, block_size_n);
        utils::set_param(encoder, 14, block_size_k);

        let thread_group_size = MTLSize {
            width: 32,
            height: 1,
            depth: 1,
        };
        let thread_groups_count = MTLSize {
            width: (size_n as u64 + BLOCK_N - 1) / BLOCK_N,
            height: (size_m as u64 + BLOCK_M - 1) / BLOCK_M,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// WNA16 MoE GEMM (INT4/INT8 weight-only)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_wna16_moe_gemm(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    input: &Buffer,
    input_offset: usize,
    weights: &Buffer,
    weights_offset: usize,
    weight_scales: &Buffer,
    weight_scales_offset: usize,
    sorted_token_ids: &Buffer,
    sorted_token_ids_offset: usize,
    expert_ids: &Buffer,
    expert_ids_offset: usize,
    topk_weights: Option<(&Buffer, usize)>,
    output: &Buffer,
    output_offset: usize,
    num_experts: i32,
    topk: i32,
    size_m: i32,
    size_n: i32,
    size_k: i32,
    bits: i32,
    group_size: i32,
    zero_point: i32,
    is_prefill: bool,
) -> Result<(), MetalKernelError> {
    if bits != 4 && bits != 8 {
        return Err(MetalKernelError::LoadFunctionError(format!(
            "wna16_moe_gemm: bits must be 4 or 8, got {bits}"
        )));
    }

    let use_gemv = !is_prefill && size_m <= 128;
    let has_topk_weights: i32 = if topk_weights.is_some() { 1 } else { 0 };
    let dummy_offset: usize = 0;

    let name = match (use_gemv, ty, bits) {
        (true, DType::F16, 4) => "wna16_moe_gemv_half_4",
        (true, DType::F16, 8) => "wna16_moe_gemv_half_8",
        (true, DType::BF16, 4) => "wna16_moe_gemv_bfloat16_4",
        (true, DType::BF16, 8) => "wna16_moe_gemv_bfloat16_8",
        (false, DType::F16, 4) => "wna16_moe_gemm_half_4_32_32",
        (false, DType::F16, 8) => "wna16_moe_gemm_half_8_32_32",
        (false, DType::BF16, 4) => "wna16_moe_gemm_bfloat16_4_32_32",
        (false, DType::BF16, 8) => "wna16_moe_gemm_bfloat16_8_32_32",
        (_, other, _) => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: other,
            })
        }
    };

    let pipeline = kernels.load_pipeline(device, name.to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(input), input_offset as NSUInteger);
    encoder.set_buffer(1, Some(weights), weights_offset as NSUInteger);
    encoder.set_buffer(2, Some(weight_scales), weight_scales_offset as NSUInteger);
    encoder.set_buffer(
        3,
        Some(sorted_token_ids),
        sorted_token_ids_offset as NSUInteger,
    );
    encoder.set_buffer(4, Some(expert_ids), expert_ids_offset as NSUInteger);
    if let Some((tw_buf, tw_off)) = topk_weights {
        encoder.set_buffer(5, Some(tw_buf), tw_off as NSUInteger);
    } else {
        encoder.set_buffer(5, Some(output), dummy_offset as NSUInteger);
    }
    encoder.set_buffer(6, Some(output), output_offset as NSUInteger);
    utils::set_param(encoder, 7, num_experts);
    utils::set_param(encoder, 8, topk);
    utils::set_param(encoder, 9, size_m);
    utils::set_param(encoder, 10, size_n);
    utils::set_param(encoder, 11, size_k);
    utils::set_param(encoder, 12, group_size);
    utils::set_param(encoder, 13, zero_point);
    utils::set_param(encoder, 14, has_topk_weights);

    let thread_group_size = MTLSize {
        width: 32,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = if use_gemv {
        MTLSize {
            width: size_n as u64,
            height: size_m as u64,
            depth: 1,
        }
    } else {
        const BLOCK_M: u64 = 32;
        const BLOCK_N: u64 = 32;
        MTLSize {
            width: (size_n as u64 + BLOCK_N - 1) / BLOCK_N,
            height: (size_m as u64 + BLOCK_M - 1) / BLOCK_M,
            depth: 1,
        }
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

// ---------------------------------------------------------------------------
// MXFP4 matmul
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_mxfp4_matmul(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    x: (&Buffer, usize),
    w: (&Buffer, usize),
    scales: (&Buffer, usize),
    bias: (&Buffer, usize),
    out: &Buffer,
    m: usize,
    n: usize,
    k: usize,
    has_bias: bool,
) -> Result<(), MetalKernelError> {
    let name = match ty {
        DType::F16 => "mxfp4_matmul_f16",
        DType::BF16 => "mxfp4_matmul_bf16",
        other => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: other,
            })
        }
    };

    let pipeline = kernels.load_pipeline(device, name.to_string())?;

    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(x.0), x.1 as NSUInteger);
    encoder.set_buffer(1, Some(w.0), w.1 as NSUInteger);
    encoder.set_buffer(2, Some(scales.0), scales.1 as NSUInteger);
    encoder.set_buffer(3, Some(bias.0), bias.1 as NSUInteger);
    encoder.set_buffer(4, Some(out), 0 as NSUInteger);
    utils::set_param(encoder, 5, m as i32);
    utils::set_param(encoder, 6, n as i32);
    utils::set_param(encoder, 7, k as i32);
    utils::set_param(encoder, 8, has_bias as i32);

    // 8 simdgroups * 32 = 256 threads per threadgroup
    let group_dims = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let grid_dims = MTLSize {
        width: ((n + 7) / 8) as u64,
        height: m as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(grid_dims, group_dims);
    Ok(())
}

// ---------------------------------------------------------------------------
// NVFP4 matmul
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_nvfp4_matmul(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    x: (&Buffer, usize),
    w: (&Buffer, usize),
    scales: (&Buffer, usize),
    bias: (&Buffer, usize),
    out: &Buffer,
    m: usize,
    n: usize,
    k: usize,
    global_scale: f32,
    has_bias: bool,
) -> Result<(), MetalKernelError> {
    let name = match ty {
        DType::F16 => "nvfp4_matmul_f16",
        DType::BF16 => "nvfp4_matmul_bf16",
        other => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: other,
            })
        }
    };

    let pipeline = kernels.load_pipeline(device, name.to_string())?;

    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(x.0), x.1 as NSUInteger);
    encoder.set_buffer(1, Some(w.0), w.1 as NSUInteger);
    encoder.set_buffer(2, Some(scales.0), scales.1 as NSUInteger);
    encoder.set_buffer(3, Some(bias.0), bias.1 as NSUInteger);
    encoder.set_buffer(4, Some(out), 0 as NSUInteger);
    utils::set_param(encoder, 5, m as i32);
    utils::set_param(encoder, 6, n as i32);
    utils::set_param(encoder, 7, k as i32);
    utils::set_param(encoder, 8, global_scale);
    utils::set_param(encoder, 9, has_bias as i32);

    let group_dims = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let grid_dims = MTLSize {
        width: ((n + 7) / 8) as u64,
        height: m as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(grid_dims, group_dims);
    Ok(())
}

// ---------------------------------------------------------------------------
// MXFP4 MoE GEMM
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_mxfp4_moe_gemm(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    x: (&Buffer, usize),
    w: (&Buffer, usize),
    scales: (&Buffer, usize),
    biases: (&Buffer, usize),
    indices: (&Buffer, usize),
    topk_weights: Option<(&Buffer, usize)>,
    out: &Buffer,
    num_tokens: usize,
    topk: usize,
    num_experts: usize,
    n: usize,
    k: usize,
    has_bias: bool,
    input_has_topk_dim: bool,
    reuse_topk: bool,
) -> Result<(), MetalKernelError> {
    let name = match (reuse_topk, ty) {
        (true, DType::F16) => "mxfp4_moe_gemm_reuse_f16",
        (true, DType::BF16) => "mxfp4_moe_gemm_reuse_bf16",
        (false, DType::F16) => "mxfp4_moe_gemm_split_f16",
        (false, DType::BF16) => "mxfp4_moe_gemm_split_bf16",
        (_, other) => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: other,
            })
        }
    };

    let has_topk_w: i32 = if topk_weights.is_some() { 1 } else { 0 };

    let pipeline = kernels.load_pipeline(device, name.to_string())?;

    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(x.0), x.1 as NSUInteger);
    encoder.set_buffer(1, Some(w.0), w.1 as NSUInteger);
    encoder.set_buffer(2, Some(scales.0), scales.1 as NSUInteger);
    encoder.set_buffer(3, Some(biases.0), biases.1 as NSUInteger);
    encoder.set_buffer(4, Some(indices.0), indices.1 as NSUInteger);
    if let Some((tw_buf, tw_off)) = topk_weights {
        encoder.set_buffer(5, Some(tw_buf), tw_off as NSUInteger);
    } else {
        encoder.set_buffer(5, Some(out), 0 as NSUInteger);
    }
    encoder.set_buffer(6, Some(out), 0 as NSUInteger);

    if reuse_topk {
        utils::set_param(encoder, 7, num_tokens as i32);
        utils::set_param(encoder, 8, topk as i32);
        utils::set_param(encoder, 9, num_experts as i32);
        utils::set_param(encoder, 10, n as i32);
        utils::set_param(encoder, 11, k as i32);
        utils::set_param(encoder, 12, has_bias as i32);
        utils::set_param(encoder, 13, has_topk_w);
    } else {
        utils::set_param(encoder, 7, num_tokens as i32);
        utils::set_param(encoder, 8, topk as i32);
        utils::set_param(encoder, 9, num_experts as i32);
        utils::set_param(encoder, 10, n as i32);
        utils::set_param(encoder, 11, k as i32);
        utils::set_param(encoder, 12, has_bias as i32);
        utils::set_param(encoder, 13, input_has_topk_dim as i32);
        utils::set_param(encoder, 14, has_topk_w);
    }

    let group_dims = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let grid_dims = MTLSize {
        width: ((n + 7) / 8) as u64,
        height: num_tokens as u64,
        depth: if reuse_topk { 1 } else { topk as u64 },
    };
    encoder.dispatch_thread_groups(grid_dims, group_dims);
    Ok(())
}

// ============================================================================
// Flash Attention Metal Kernels
// ============================================================================

#[allow(clippy::too_many_arguments)]
pub fn flash_attention_prefill(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_cache: &Buffer,
    k_cache_offset: usize,
    v_cache: &Buffer,
    v_cache_offset: usize,
    block_tables: &Buffer,
    block_tables_offset: usize,
    seq_lens: &Buffer,
    seq_lens_offset: usize,
    query_start_len: &Buffer,
    query_start_len_offset: usize,
    alibi_slopes: Option<(MetalStorage, usize)>,
    k_scale: Option<MetalStorage>,
    v_scale: Option<MetalStorage>,
    sinks: Option<(MetalStorage, usize)>,
    num_kv_heads: i32,
    scale: f32,
    block_table_stride: i32,
    num_seqs: i32,
    num_query_heads: i32,
    num_query_tokens: i32,
    head_size: i32,
    block_size: i32,
    softcapping: f32,
    o_stride_tokens: i32,
    sliding_window: i32,
    total_num_blocks: i32,
    kv_block_stride: i32,
    kv_head_stride: i32,
) -> Result<(), MetalKernelError> {
    const BR: u64 = 64;
    let quantized_cache = k_scale.is_some() && v_scale.is_some();

    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };

    let fp8_suffix = if quantized_cache { "_fp8" } else { "" };
    let name = format!(
        "flash_prefill_{}{}_hd{}_bs{}",
        type_prefix, fp8_suffix, head_size, block_size
    );

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_cache), k_cache_offset as NSUInteger);
    encoder.set_buffer(3, Some(v_cache), v_cache_offset as NSUInteger);
    encoder.set_bytes(
        4,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(5, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_buffer(6, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(7, Some(seq_lens), seq_lens_offset as NSUInteger);
    encoder.set_bytes(
        8,
        size_of_val(&block_table_stride),
        &block_table_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        9,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        10,
        size_of_val(&num_query_heads),
        &num_query_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        11,
        size_of_val(&num_query_tokens),
        &num_query_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        12,
        size_of_val(&softcapping),
        &softcapping as *const _ as *const c_void,
    );
    encoder.set_bytes(
        13,
        size_of_val(&o_stride_tokens),
        &o_stride_tokens as *const _ as *const c_void,
    );
    encoder.set_buffer(
        14,
        Some(query_start_len),
        query_start_len_offset as NSUInteger,
    );
    if let Some((slop, offset)) = alibi_slopes {
        encoder.set_buffer(15, Some(slop.buffer()), offset as NSUInteger);
    }
    if let Some(ks) = k_scale {
        encoder.set_buffer(16, Some(ks.buffer()), 0);
    }
    if let Some(vs) = v_scale {
        encoder.set_buffer(17, Some(vs.buffer()), 0);
    }
    if let Some((sk, offset)) = sinks {
        encoder.set_buffer(18, Some(sk.buffer()), offset as NSUInteger);
    }
    encoder.set_bytes(
        19,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );
    encoder.set_bytes(
        20,
        size_of_val(&total_num_blocks),
        &total_num_blocks as *const _ as *const c_void,
    );
    encoder.set_bytes(
        21,
        size_of_val(&kv_block_stride),
        &kv_block_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        22,
        size_of_val(&kv_head_stride),
        &kv_head_stride as *const _ as *const c_void,
    );

    let num_queries_per_kv = (num_query_heads / num_kv_heads) as u64;
    let num_token_chunks = (num_query_tokens as u64 + BR - 1) / BR;

    let thread_group_size = MTLSize {
        width: BR,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: num_queries_per_kv,
        height: num_kv_heads as u64,
        depth: num_token_chunks,
    };

    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_attention_decode(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_cache: &Buffer,
    k_cache_offset: usize,
    v_cache: &Buffer,
    v_cache_offset: usize,
    block_tables: &Buffer,
    block_tables_offset: usize,
    context_lens: &Buffer,
    context_lens_offset: usize,
    k_scale: Option<MetalStorage>,
    v_scale: Option<MetalStorage>,
    num_q_heads: i32,
    num_kv_heads: i32,
    scale: f32,
    softcapping: f32,
    block_size: i32,
    _max_context_len: i32,
    num_seqs: i32,
    head_size: i32,
    max_num_blocks_per_seq: i32,
    q_stride: i32,
    sliding_window: i32,
) -> Result<(), MetalKernelError> {
    const NUM_THREADS: u64 = 256;
    let quantized_cache = k_scale.is_some() && v_scale.is_some();

    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };

    let fp8_suffix = if quantized_cache { "_fp8" } else { "" };
    let name = format!(
        "flash_decode_{}{}_hd{}_bs{}",
        type_prefix, fp8_suffix, head_size, block_size
    );

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    // Shared memory: q_shared(HEAD_DIM), followed by the larger of the
    // s_m/s_l reduction arrays and s_reduce. The decode kernel still loads Q
    // into threadgroup memory before using the reduction workspace.
    let num_simd_groups = (NUM_THREADS / 32) as u64;
    let reduction_size = (NUM_THREADS * 2).max(num_simd_groups * head_size as u64);
    let smem_size = (head_size as u64 + reduction_size) * std::mem::size_of::<f32>() as u64;
    encoder.set_threadgroup_memory_length(0, smem_size);

    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_cache), k_cache_offset as NSUInteger);
    encoder.set_buffer(3, Some(v_cache), v_cache_offset as NSUInteger);
    encoder.set_buffer(4, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(5, Some(context_lens), context_lens_offset as NSUInteger);
    encoder.set_bytes(
        6,
        size_of_val(&max_num_blocks_per_seq),
        &max_num_blocks_per_seq as *const _ as *const c_void,
    );
    encoder.set_bytes(
        7,
        size_of_val(&num_q_heads),
        &num_q_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        8,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        9,
        size_of_val(&head_size),
        &head_size as *const _ as *const c_void,
    );
    encoder.set_bytes(
        10,
        size_of_val(&block_size),
        &block_size as *const _ as *const c_void,
    );
    encoder.set_bytes(11, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_bytes(
        12,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        13,
        size_of_val(&q_stride),
        &q_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        14,
        size_of_val(&softcapping),
        &softcapping as *const _ as *const c_void,
    );
    encoder.set_bytes(
        15,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );
    if let Some(ks) = k_scale {
        encoder.set_buffer(16, Some(ks.buffer()), 0);
    }
    if let Some(vs) = v_scale {
        encoder.set_buffer(17, Some(vs.buffer()), 0);
    }

    let thread_groups_count = MTLSize {
        width: num_q_heads as u64,
        height: num_seqs as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: NUM_THREADS,
        height: 1,
        depth: 1,
    };

    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_attention_decode_splitk(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    partial_out: &Buffer,
    partial_max: &Buffer,
    partial_sum: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_cache: &Buffer,
    k_cache_offset: usize,
    v_cache: &Buffer,
    v_cache_offset: usize,
    block_tables: &Buffer,
    block_tables_offset: usize,
    context_lens: &Buffer,
    context_lens_offset: usize,
    k_scale: Option<MetalStorage>,
    v_scale: Option<MetalStorage>,
    num_q_heads: i32,
    num_kv_heads: i32,
    scale: f32,
    softcapping: f32,
    block_size: i32,
    num_seqs: i32,
    head_size: i32,
    max_num_blocks_per_seq: i32,
    q_stride: i32,
    sliding_window: i32,
    num_splits: i32,
) -> Result<(), MetalKernelError> {
    const NUM_THREADS: u64 = 256;
    let quantized_cache = k_scale.is_some() && v_scale.is_some();

    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let fp8_suffix = if quantized_cache { "_fp8" } else { "" };

    // Phase 1: Split-K kernel
    {
        let name = format!(
            "flash_decode_splitk_{}{}_hd{}_bs{}",
            type_prefix, fp8_suffix, head_size, block_size
        );
        let pipeline = kernels.load_pipeline(device, name)?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        // s_max(NUM_THREADS) + s_sum(NUM_THREADS) + s_reduce(NUM_SIMD_GROUPS * HEAD_DIM)
        let num_simd_groups = NUM_THREADS / 32;
        let smem_size = (NUM_THREADS * 2 + num_simd_groups * head_size as u64)
            * std::mem::size_of::<f32>() as u64;
        encoder.set_threadgroup_memory_length(0, smem_size);

        encoder.set_buffer(0, Some(partial_out), 0);
        encoder.set_buffer(1, Some(partial_max), 0);
        encoder.set_buffer(2, Some(partial_sum), 0);
        encoder.set_buffer(3, Some(q), q_offset as NSUInteger);
        encoder.set_buffer(4, Some(k_cache), k_cache_offset as NSUInteger);
        encoder.set_buffer(5, Some(v_cache), v_cache_offset as NSUInteger);
        encoder.set_buffer(6, Some(block_tables), block_tables_offset as NSUInteger);
        encoder.set_buffer(7, Some(context_lens), context_lens_offset as NSUInteger);
        encoder.set_bytes(
            8,
            size_of_val(&max_num_blocks_per_seq),
            &max_num_blocks_per_seq as *const _ as *const c_void,
        );
        encoder.set_bytes(
            9,
            size_of_val(&num_q_heads),
            &num_q_heads as *const _ as *const c_void,
        );
        encoder.set_bytes(
            10,
            size_of_val(&num_kv_heads),
            &num_kv_heads as *const _ as *const c_void,
        );
        encoder.set_bytes(
            11,
            size_of_val(&head_size),
            &head_size as *const _ as *const c_void,
        );
        encoder.set_bytes(
            12,
            size_of_val(&block_size),
            &block_size as *const _ as *const c_void,
        );
        encoder.set_bytes(13, size_of_val(&scale), &scale as *const _ as *const c_void);
        encoder.set_bytes(
            14,
            size_of_val(&num_seqs),
            &num_seqs as *const _ as *const c_void,
        );
        encoder.set_bytes(
            15,
            size_of_val(&num_splits),
            &num_splits as *const _ as *const c_void,
        );
        encoder.set_bytes(
            16,
            size_of_val(&q_stride),
            &q_stride as *const _ as *const c_void,
        );
        encoder.set_bytes(
            17,
            size_of_val(&softcapping),
            &softcapping as *const _ as *const c_void,
        );
        encoder.set_bytes(
            18,
            size_of_val(&sliding_window),
            &sliding_window as *const _ as *const c_void,
        );
        if let Some(ks) = &k_scale {
            encoder.set_buffer(19, Some(ks.buffer()), 0);
        }
        if let Some(vs) = &v_scale {
            encoder.set_buffer(20, Some(vs.buffer()), 0);
        }

        let thread_groups_count = MTLSize {
            width: num_q_heads as u64,
            height: num_seqs as u64,
            depth: num_splits as u64,
        };
        let thread_group_size = MTLSize {
            width: NUM_THREADS,
            height: 1,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }

    // Phase 2: Reduce kernel
    {
        let name = format!("flash_decode_reduce_{}_hd{}", type_prefix, head_size);
        let pipeline = kernels.load_pipeline(device, name)?;
        let encoder = ep.encoder();
        let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
        encoder.set_compute_pipeline_state(&pipeline);

        encoder.set_buffer(0, Some(output), 0);
        encoder.set_buffer(1, Some(partial_out), 0);
        encoder.set_buffer(2, Some(partial_max), 0);
        encoder.set_buffer(3, Some(partial_sum), 0);
        encoder.set_bytes(
            4,
            size_of_val(&num_q_heads),
            &num_q_heads as *const _ as *const c_void,
        );
        encoder.set_bytes(
            5,
            size_of_val(&num_splits),
            &num_splits as *const _ as *const c_void,
        );
        encoder.set_bytes(
            6,
            size_of_val(&q_stride),
            &q_stride as *const _ as *const c_void,
        );

        let thread_groups_count = MTLSize {
            width: num_q_heads as u64,
            height: num_seqs as u64,
            depth: 1,
        };
        let thread_group_size = MTLSize {
            width: head_size as u64,
            height: 1,
            depth: 1,
        };
        encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_reshape_and_cache_metal(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    key: &Buffer,
    key_offset: usize,
    value: &Buffer,
    value_offset: usize,
    key_cache: &Buffer,
    key_cache_offset: usize,
    value_cache: &Buffer,
    value_cache_offset: usize,
    slot_mapping: &Buffer,
    slot_mapping_offset: usize,
    num_tokens: i32,
    num_heads: i32,
    head_size: i32,
    block_size: i32,
    key_stride: i32,
    value_stride: i32,
    k_scale: Option<Buffer>,
    v_scale: Option<Buffer>,
) -> Result<(), MetalKernelError> {
    let quantized_cache = k_scale.is_some() && v_scale.is_some();

    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "f32",
    };
    let cache_suffix = if quantized_cache {
        "_fp8"
    } else {
        match ty {
            PagedAttentionDType::BF16 => "_bf16",
            PagedAttentionDType::F16 => "_f16",
            PagedAttentionDType::F32 => "_f32",
        }
    };
    let name = format!("flash_reshape_cache_{}{}", type_prefix, cache_suffix);

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(key), key_offset as NSUInteger);
    encoder.set_buffer(1, Some(value), value_offset as NSUInteger);
    encoder.set_buffer(2, Some(key_cache), key_cache_offset as NSUInteger);
    encoder.set_buffer(3, Some(value_cache), value_cache_offset as NSUInteger);
    encoder.set_buffer(4, Some(slot_mapping), slot_mapping_offset as NSUInteger);
    encoder.set_bytes(
        5,
        size_of_val(&num_tokens) as NSUInteger,
        &num_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        6,
        size_of_val(&num_heads) as NSUInteger,
        &num_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        7,
        size_of_val(&head_size) as NSUInteger,
        &head_size as *const _ as *const c_void,
    );
    encoder.set_bytes(
        8,
        size_of_val(&block_size) as NSUInteger,
        &block_size as *const _ as *const c_void,
    );
    encoder.set_bytes(
        9,
        size_of_val(&key_stride) as NSUInteger,
        &key_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        10,
        size_of_val(&value_stride) as NSUInteger,
        &value_stride as *const _ as *const c_void,
    );
    if let Some(ks) = &k_scale {
        encoder.set_buffer(11, Some(ks), 0);
    }
    if let Some(vs) = &v_scale {
        encoder.set_buffer(12, Some(vs), 0);
    }
    let thread_groups_count = MTLSize {
        width: num_tokens as u64,
        height: 1,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };

    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_tq_store_k8v4(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    value: &Buffer,
    value_offset: usize,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    slot_mapping: &Buffer,
    slot_mapping_offset: usize,
    num_tokens: i32,
    num_kv_heads: i32,
    head_dim: i32,
    block_size: i32,
) -> Result<(), MetalKernelError> {
    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let hd_str = match head_dim {
        128 => "hd128",
        256 => "hd256",
        _ => "hd128",
    };
    let name = format!("flash_tq_store_k8v4_{}_{}", type_prefix, hd_str);

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(value), value_offset as NSUInteger);
    encoder.set_buffer(1, Some(v_absmax), 0);
    encoder.set_buffer(2, Some(v_quant), 0);
    encoder.set_buffer(3, Some(slot_mapping), slot_mapping_offset as NSUInteger);
    encoder.set_bytes(
        4,
        size_of_val(&num_tokens),
        &num_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        5,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        6,
        size_of_val(&block_size),
        &block_size as *const _ as *const c_void,
    );

    let thread_groups_count = MTLSize {
        width: num_tokens as u64,
        height: num_kv_heads as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };

    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_tq_decode_k8v4(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_cache: &Buffer,
    k_cache_offset: usize,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    block_tables: &Buffer,
    block_tables_offset: usize,
    context_lens: &Buffer,
    context_lens_offset: usize,
    k_scale: Option<MetalStorage>,
    num_q_heads: i32,
    num_kv_heads: i32,
    scale: f32,
    softcap: f32,
    block_size: i32,
    num_seqs: i32,
    head_dim: i32,
    max_blocks_per_seq: i32,
    q_stride: i32,
    sliding_window: i32,
) -> Result<(), MetalKernelError> {
    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let hd_str = match head_dim {
        128 => "hd128",
        256 => "hd256",
        _ => "hd128",
    };
    let name = format!("flash_tq_decode_k8v4_{}_{}_{}", type_prefix, hd_str, "bs32");

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_cache), k_cache_offset as NSUInteger);
    encoder.set_buffer(3, Some(v_absmax), 0);
    encoder.set_buffer(4, Some(v_quant), 0);
    encoder.set_buffer(5, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(6, Some(context_lens), context_lens_offset as NSUInteger);

    if let Some(ks) = &k_scale {
        encoder.set_buffer(7, Some(ks.buffer()), 0);
    }
    encoder.set_bytes(8, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_bytes(
        9,
        size_of_val(&softcap),
        &softcap as *const _ as *const c_void,
    );
    encoder.set_bytes(
        10,
        size_of_val(&num_q_heads),
        &num_q_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        11,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        12,
        size_of_val(&block_size),
        &block_size as *const _ as *const c_void,
    );

    let max_context_len: i32 = 0;
    encoder.set_bytes(
        13,
        size_of_val(&max_context_len),
        &max_context_len as *const _ as *const c_void,
    );
    encoder.set_bytes(
        14,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        15,
        size_of_val(&head_dim),
        &head_dim as *const _ as *const c_void,
    );
    encoder.set_bytes(
        16,
        size_of_val(&max_blocks_per_seq),
        &max_blocks_per_seq as *const _ as *const c_void,
    );
    encoder.set_bytes(
        17,
        size_of_val(&q_stride),
        &q_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        18,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );

    let thread_groups_count = MTLSize {
        width: num_seqs as u64,
        height: num_q_heads as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };

    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_tq4_store(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    key: &Buffer,
    key_offset: usize,
    value: &Buffer,
    value_offset: usize,
    k_absmax: &Buffer,
    k_quant: &Buffer,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    slot_mapping: &Buffer,
    slot_mapping_offset: usize,
    num_tokens: i32,
    num_kv_heads: i32,
    head_dim: i32,
    block_size: i32,
) -> Result<(), MetalKernelError> {
    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let hd_str = match head_dim {
        256 => "hd256",
        _ => "hd128",
    };
    let name = format!("flash_tq4_store_{}_{}", type_prefix, hd_str);

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(key), key_offset as NSUInteger);
    encoder.set_buffer(1, Some(value), value_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_absmax), 0);
    encoder.set_buffer(3, Some(k_quant), 0);
    encoder.set_buffer(4, Some(v_absmax), 0);
    encoder.set_buffer(5, Some(v_quant), 0);
    encoder.set_buffer(6, Some(slot_mapping), slot_mapping_offset as NSUInteger);
    encoder.set_bytes(
        7,
        size_of_val(&num_tokens),
        &num_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        8,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        9,
        size_of_val(&block_size),
        &block_size as *const _ as *const c_void,
    );

    let thread_groups_count = MTLSize {
        width: num_tokens as u64,
        height: num_kv_heads as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 32,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_tq4_decode(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_absmax: &Buffer,
    k_quant: &Buffer,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    block_tables: &Buffer,
    block_tables_offset: usize,
    context_lens: &Buffer,
    context_lens_offset: usize,
    scale: f32,
    softcap: f32,
    num_q_heads: i32,
    num_kv_heads: i32,
    block_size: i32,
    num_seqs: i32,
    head_dim: i32,
    max_blocks_per_seq: i32,
    q_stride: i32,
    sliding_window: i32,
) -> Result<(), MetalKernelError> {
    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let hd_str = match head_dim {
        256 => "hd256",
        _ => "hd128",
    };
    let name = format!("flash_tq4_decode_{}_{}_{}", type_prefix, hd_str, "bs32");

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_absmax), 0);
    encoder.set_buffer(3, Some(k_quant), 0);
    encoder.set_buffer(4, Some(v_absmax), 0);
    encoder.set_buffer(5, Some(v_quant), 0);
    encoder.set_buffer(6, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(7, Some(context_lens), context_lens_offset as NSUInteger);
    encoder.set_bytes(8, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_bytes(
        9,
        size_of_val(&softcap),
        &softcap as *const _ as *const c_void,
    );
    encoder.set_bytes(
        10,
        size_of_val(&num_q_heads),
        &num_q_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        11,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        12,
        size_of_val(&block_size),
        &block_size as *const _ as *const c_void,
    );
    encoder.set_bytes(
        13,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        14,
        size_of_val(&head_dim),
        &head_dim as *const _ as *const c_void,
    );
    encoder.set_bytes(
        15,
        size_of_val(&max_blocks_per_seq),
        &max_blocks_per_seq as *const _ as *const c_void,
    );
    encoder.set_bytes(
        16,
        size_of_val(&q_stride),
        &q_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        17,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );

    let thread_groups_count = MTLSize {
        width: num_seqs as u64,
        height: num_q_heads as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

pub fn flash_tq4_prefill(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_absmax: &Buffer,
    k_quant: &Buffer,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    block_tables: &Buffer,
    block_tables_offset: usize,
    seq_lens: &Buffer,
    seq_lens_offset: usize,
    query_start_len: &Buffer,
    query_start_len_offset: usize,
    num_kv_heads: i32,
    scale: f32,
    block_table_stride: i32,
    num_seqs: i32,
    num_query_heads: i32,
    num_query_tokens: i32,
    head_size: i32,
    block_size: i32,
    softcapping: f32,
    o_stride_tokens: i32,
    sliding_window: i32,
) -> Result<(), MetalKernelError> {
    const BR: u64 = 8;

    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let name = format!(
        "flash_tq4_prefill_{}_hd{}_bs{}",
        type_prefix, head_size, block_size
    );

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_absmax), 0);
    encoder.set_buffer(3, Some(k_quant), 0);
    encoder.set_buffer(4, Some(v_absmax), 0);
    encoder.set_buffer(5, Some(v_quant), 0);
    encoder.set_bytes(
        6,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(7, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_buffer(8, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(9, Some(seq_lens), seq_lens_offset as NSUInteger);
    encoder.set_bytes(
        10,
        size_of_val(&block_table_stride),
        &block_table_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        11,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        12,
        size_of_val(&num_query_heads),
        &num_query_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        13,
        size_of_val(&num_query_tokens),
        &num_query_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        14,
        size_of_val(&softcapping),
        &softcapping as *const _ as *const c_void,
    );
    encoder.set_bytes(
        15,
        size_of_val(&o_stride_tokens),
        &o_stride_tokens as *const _ as *const c_void,
    );
    encoder.set_buffer(
        16,
        Some(query_start_len),
        query_start_len_offset as NSUInteger,
    );
    encoder.set_bytes(
        17,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );

    let num_groups_per_kv = num_query_heads / num_kv_heads;
    let thread_groups_count = MTLSize {
        width: num_groups_per_kv as u64,
        height: num_kv_heads as u64,
        depth: (num_query_tokens as u64 + BR - 1) / BR,
    };
    let thread_group_size = MTLSize {
        width: BR,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

// ---------------------------------------------------------------------------
// TurboQuant Turbo3: 3-bit K + 4-bit V (Metal kernels)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn flash_tq3_store(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    key: &Buffer,
    key_offset: usize,
    value: &Buffer,
    value_offset: usize,
    k_absmax: &Buffer,
    k_quant: &Buffer,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    slot_mapping: &Buffer,
    slot_mapping_offset: usize,
    num_tokens: i32,
    num_kv_heads: i32,
    head_dim: i32,
    block_size: i32,
) -> Result<(), MetalKernelError> {
    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let hd_str = match head_dim {
        256 => "hd256",
        _ => "hd128",
    };
    let name = format!("flash_tq3_store_{}_{}", type_prefix, hd_str);

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(key), key_offset as NSUInteger);
    encoder.set_buffer(1, Some(value), value_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_absmax), 0);
    encoder.set_buffer(3, Some(k_quant), 0);
    encoder.set_buffer(4, Some(v_absmax), 0);
    encoder.set_buffer(5, Some(v_quant), 0);
    encoder.set_buffer(6, Some(slot_mapping), slot_mapping_offset as NSUInteger);
    encoder.set_bytes(
        7,
        size_of_val(&num_tokens),
        &num_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        8,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        9,
        size_of_val(&block_size),
        &block_size as *const _ as *const c_void,
    );

    let thread_groups_count = MTLSize {
        width: num_tokens as u64,
        height: num_kv_heads as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 32,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_tq3_decode(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_absmax: &Buffer,
    k_quant: &Buffer,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    block_tables: &Buffer,
    block_tables_offset: usize,
    context_lens: &Buffer,
    context_lens_offset: usize,
    scale: f32,
    softcap: f32,
    num_q_heads: i32,
    num_kv_heads: i32,
    block_size: i32,
    num_seqs: i32,
    head_dim: i32,
    max_blocks_per_seq: i32,
    q_stride: i32,
    sliding_window: i32,
) -> Result<(), MetalKernelError> {
    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let hd_str = match head_dim {
        256 => "hd256",
        _ => "hd128",
    };
    let name = format!("flash_tq3_decode_{}_{}_{}", type_prefix, hd_str, "bs32");

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_absmax), 0);
    encoder.set_buffer(3, Some(k_quant), 0);
    encoder.set_buffer(4, Some(v_absmax), 0);
    encoder.set_buffer(5, Some(v_quant), 0);
    encoder.set_buffer(6, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(7, Some(context_lens), context_lens_offset as NSUInteger);
    encoder.set_bytes(8, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_bytes(
        9,
        size_of_val(&softcap),
        &softcap as *const _ as *const c_void,
    );
    encoder.set_bytes(
        10,
        size_of_val(&num_q_heads),
        &num_q_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        11,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        12,
        size_of_val(&block_size),
        &block_size as *const _ as *const c_void,
    );
    encoder.set_bytes(
        13,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        14,
        size_of_val(&head_dim),
        &head_dim as *const _ as *const c_void,
    );
    encoder.set_bytes(
        15,
        size_of_val(&max_blocks_per_seq),
        &max_blocks_per_seq as *const _ as *const c_void,
    );
    encoder.set_bytes(
        16,
        size_of_val(&q_stride),
        &q_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        17,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );

    let thread_groups_count = MTLSize {
        width: num_seqs as u64,
        height: num_q_heads as u64,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn flash_tq3_prefill(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: PagedAttentionDType,
    output: &Buffer,
    q: &Buffer,
    q_offset: usize,
    k_absmax: &Buffer,
    k_quant: &Buffer,
    v_absmax: &Buffer,
    v_quant: &Buffer,
    block_tables: &Buffer,
    block_tables_offset: usize,
    seq_lens: &Buffer,
    seq_lens_offset: usize,
    query_start_len: &Buffer,
    query_start_len_offset: usize,
    num_kv_heads: i32,
    scale: f32,
    block_table_stride: i32,
    num_seqs: i32,
    num_query_heads: i32,
    num_query_tokens: i32,
    head_size: i32,
    block_size: i32,
    softcapping: f32,
    o_stride_tokens: i32,
    sliding_window: i32,
) -> Result<(), MetalKernelError> {
    const BR: u64 = 8;

    let type_prefix = match ty {
        PagedAttentionDType::BF16 => "bf16",
        PagedAttentionDType::F16 => "f16",
        PagedAttentionDType::F32 => "bf16",
    };
    let name = format!(
        "flash_tq3_prefill_{}_hd{}_bs{}",
        type_prefix, head_size, block_size
    );

    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);

    encoder.set_buffer(0, Some(output), 0);
    encoder.set_buffer(1, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(2, Some(k_absmax), 0);
    encoder.set_buffer(3, Some(k_quant), 0);
    encoder.set_buffer(4, Some(v_absmax), 0);
    encoder.set_buffer(5, Some(v_quant), 0);
    encoder.set_bytes(
        6,
        size_of_val(&num_kv_heads),
        &num_kv_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(7, size_of_val(&scale), &scale as *const _ as *const c_void);
    encoder.set_buffer(8, Some(block_tables), block_tables_offset as NSUInteger);
    encoder.set_buffer(9, Some(seq_lens), seq_lens_offset as NSUInteger);
    encoder.set_bytes(
        10,
        size_of_val(&block_table_stride),
        &block_table_stride as *const _ as *const c_void,
    );
    encoder.set_bytes(
        11,
        size_of_val(&num_seqs),
        &num_seqs as *const _ as *const c_void,
    );
    encoder.set_bytes(
        12,
        size_of_val(&num_query_heads),
        &num_query_heads as *const _ as *const c_void,
    );
    encoder.set_bytes(
        13,
        size_of_val(&num_query_tokens),
        &num_query_tokens as *const _ as *const c_void,
    );
    encoder.set_bytes(
        14,
        size_of_val(&softcapping),
        &softcapping as *const _ as *const c_void,
    );
    encoder.set_bytes(
        15,
        size_of_val(&o_stride_tokens),
        &o_stride_tokens as *const _ as *const c_void,
    );
    encoder.set_buffer(
        16,
        Some(query_start_len),
        query_start_len_offset as NSUInteger,
    );
    encoder.set_bytes(
        17,
        size_of_val(&sliding_window),
        &sliding_window as *const _ as *const c_void,
    );

    let num_groups_per_kv = num_query_heads / num_kv_heads;
    let thread_groups_count = MTLSize {
        width: num_groups_per_kv as u64,
        height: num_kv_heads as u64,
        depth: (num_query_tokens as u64 + BR - 1) / BR,
    };
    let thread_group_size = MTLSize {
        width: BR,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

fn gdn_type_name(ty: DType) -> Result<&'static str, MetalKernelError> {
    match ty {
        DType::F32 => Ok("float"),
        DType::F16 => Ok("half"),
        DType::BF16 => Ok("bfloat16_t"),
        other => Err(MetalKernelError::DTypeMismatch {
            expected: vec![DType::F32, DType::F16, DType::BF16],
            got: other,
        }),
    }
}

fn gdn_recurrence_kernel_name(
    base: &str,
    ty: DType,
    k_dim: i32,
) -> Result<String, MetalKernelError> {
    let ty_name = gdn_type_name(ty)?;
    let bk = match k_dim {
        64 => 64,
        128 => 128,
        _ => {
            return Err(MetalKernelError::FailedToCreatePipeline(format!(
                "{base}: unsupported k_dim={k_dim}, expected 64 or 128"
            )))
        }
    };
    Ok(format!("{base}_{ty_name}_k{bk}"))
}

fn gdn_conv_kernel_name(
    base: &str,
    ty: DType,
    kernel_size: i32,
) -> Result<String, MetalKernelError> {
    let ty_name = gdn_type_name(ty)?;
    match kernel_size {
        2..=4 => Ok(format!("{base}_{ty_name}_k{kernel_size}")),
        _ => Err(MetalKernelError::FailedToCreatePipeline(format!(
            "{base}: unsupported kernel_size={kernel_size}, expected 2, 3, or 4"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_causal_conv1d_fwd(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    x: &Buffer,
    x_offset: usize,
    weight: &Buffer,
    weight_offset: usize,
    bias: Option<(&Buffer, usize)>,
    conv_state: &Buffer,
    conv_state_offset: usize,
    out: &Buffer,
    out_offset: usize,
    state_snapshots: Option<(&Buffer, usize)>,
    cu_seqlens: &Buffer,
    cu_seqlens_offset: usize,
    batch_size: i32,
    d_conv: i32,
    kernel_size: i32,
    activation_silu: bool,
) -> Result<(), MetalKernelError> {
    let name = gdn_conv_kernel_name("gdn_causal_conv1d_fwd", ty, kernel_size)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(x), x_offset as NSUInteger);
    encoder.set_buffer(1, Some(weight), weight_offset as NSUInteger);
    if let Some((bias, offset)) = bias {
        encoder.set_buffer(2, Some(bias), offset as NSUInteger);
    } else {
        encoder.set_buffer(2, None, 0);
    }
    encoder.set_buffer(3, Some(conv_state), conv_state_offset as NSUInteger);
    encoder.set_buffer(4, Some(out), out_offset as NSUInteger);
    if let Some((snap, offset)) = state_snapshots {
        encoder.set_buffer(5, Some(snap), offset as NSUInteger);
    } else {
        encoder.set_buffer(5, None, 0);
    }
    encoder.set_buffer(6, Some(cu_seqlens), cu_seqlens_offset as NSUInteger);
    utils::set_param(encoder, 7, batch_size);
    utils::set_param(encoder, 8, d_conv);
    utils::set_param(encoder, 9, activation_silu);

    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((d_conv as u64) + 255) / 256,
        height: batch_size as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_causal_conv1d_update(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    x: &Buffer,
    x_offset: usize,
    weight: &Buffer,
    weight_offset: usize,
    bias: Option<(&Buffer, usize)>,
    conv_state: &Buffer,
    conv_state_offset: usize,
    out: &Buffer,
    out_offset: usize,
    batch_size: i32,
    d_conv: i32,
    kernel_size: i32,
    activation_silu: bool,
) -> Result<(), MetalKernelError> {
    let name = gdn_conv_kernel_name("gdn_causal_conv1d_update", ty, kernel_size)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(x), x_offset as NSUInteger);
    encoder.set_buffer(1, Some(weight), weight_offset as NSUInteger);
    if let Some((bias, offset)) = bias {
        encoder.set_buffer(2, Some(bias), offset as NSUInteger);
    } else {
        encoder.set_buffer(2, None, 0);
    }
    encoder.set_buffer(3, Some(conv_state), conv_state_offset as NSUInteger);
    encoder.set_buffer(4, Some(out), out_offset as NSUInteger);
    utils::set_param(encoder, 5, batch_size);
    utils::set_param(encoder, 6, d_conv);
    utils::set_param(encoder, 7, activation_silu);

    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((d_conv as u64) + 255) / 256,
        height: batch_size as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_causal_conv1d_update_slots(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    x: &Buffer,
    x_offset: usize,
    weight: &Buffer,
    weight_offset: usize,
    bias: Option<(&Buffer, usize)>,
    conv_state: &Buffer,
    conv_state_offset: usize,
    slots: &Buffer,
    slots_offset: usize,
    out: &Buffer,
    out_offset: usize,
    batch_size: i32,
    d_conv: i32,
    kernel_size: i32,
    activation_silu: bool,
) -> Result<(), MetalKernelError> {
    let name = gdn_conv_kernel_name("gdn_causal_conv1d_update_slots", ty, kernel_size)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(x), x_offset as NSUInteger);
    encoder.set_buffer(1, Some(weight), weight_offset as NSUInteger);
    if let Some((bias, offset)) = bias {
        encoder.set_buffer(2, Some(bias), offset as NSUInteger);
    } else {
        encoder.set_buffer(2, None, 0);
    }
    encoder.set_buffer(3, Some(conv_state), conv_state_offset as NSUInteger);
    encoder.set_buffer(4, Some(slots), slots_offset as NSUInteger);
    encoder.set_buffer(5, Some(out), out_offset as NSUInteger);
    utils::set_param(encoder, 6, batch_size);
    utils::set_param(encoder, 7, d_conv);
    utils::set_param(encoder, 8, activation_silu);

    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((d_conv as u64) + 255) / 256,
        height: batch_size as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_fused_gating(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    a_log: &Buffer,
    a_log_offset: usize,
    a: &Buffer,
    a_offset: usize,
    b: &Buffer,
    b_offset: usize,
    dt_bias: &Buffer,
    dt_bias_offset: usize,
    g: &Buffer,
    g_offset: usize,
    beta: &Buffer,
    beta_offset: usize,
    total_elements: i32,
    num_heads: i32,
) -> Result<(), MetalKernelError> {
    let name = match ty {
        DType::F32 => "gdn_fused_gating_float".to_string(),
        DType::BF16 => "gdn_fused_gating_bfloat16_t".to_string(),
        _ => {
            return Err(MetalKernelError::FailedToCreatePipeline(format!(
                "unsupported fused gating dtype: a={ty:?} (a_log and dt_bias are always F32)"
            )))
        }
    };
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            (a_log, a_log_offset),
            (a, a_offset),
            (b, b_offset),
            (dt_bias, dt_bias_offset),
            (g, g_offset),
            (beta, beta_offset),
            total_elements,
            num_heads
        )
    );

    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((total_elements as u64) + 255) / 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_l2_norm_last_dim(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    input: &Buffer,
    input_offset: usize,
    output: &Buffer,
    output_offset: usize,
    rows: i32,
    dim: i32,
    eps: f32,
) -> Result<(), MetalKernelError> {
    let name = format!("gdn_l2_norm_last_dim_{}", gdn_type_name(ty)?);
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            (input, input_offset),
            (output, output_offset),
            rows,
            dim,
            eps
        )
    );

    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: rows as u64,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_gated_rmsnorm_silu_mul(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    weight_ty: DType,
    x: &Buffer,
    x_offset: usize,
    z: &Buffer,
    z_offset: usize,
    gamma: &Buffer,
    gamma_offset: usize,
    bias: Option<(&Buffer, usize)>,
    out: &Buffer,
    out_offset: usize,
    rows: i32,
    value_dim: i32,
    group_size: i32,
    eps: f32,
    per_group_weights: bool,
    has_bias: bool,
) -> Result<(), MetalKernelError> {
    let name = match (ty, weight_ty) {
        (DType::F32, DType::F32) => "gdn_gated_rmsnorm_silu_mul_float".to_string(),
        (DType::F16, DType::F16) => "gdn_gated_rmsnorm_silu_mul_half".to_string(),
        (DType::BF16, DType::BF16) => "gdn_gated_rmsnorm_silu_mul_bfloat16_t".to_string(),
        (DType::F16, DType::F32) => "gdn_gated_rmsnorm_silu_mul_half_wf32".to_string(),
        (DType::BF16, DType::F32) => "gdn_gated_rmsnorm_silu_mul_bfloat16_t_wf32".to_string(),
        _ => {
            return Err(MetalKernelError::FailedToCreatePipeline(format!(
                "unsupported gated_rmsnorm dtypes: x={ty:?}, weight={weight_ty:?}"
            )))
        }
    };
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(x), x_offset as NSUInteger);
    encoder.set_buffer(1, Some(z), z_offset as NSUInteger);
    encoder.set_buffer(2, Some(gamma), gamma_offset as NSUInteger);
    if let Some((bias, offset)) = bias {
        encoder.set_buffer(3, Some(bias), offset as NSUInteger);
    } else {
        encoder.set_buffer(3, None, 0);
    }
    encoder.set_buffer(4, Some(out), out_offset as NSUInteger);
    utils::set_param(encoder, 5, rows);
    utils::set_param(encoder, 6, value_dim);
    utils::set_param(encoder, 7, group_size);
    utils::set_param(encoder, 8, eps);
    utils::set_param(encoder, 9, per_group_weights);
    utils::set_param(encoder, 10, has_bias);

    let num_groups = (value_dim / group_size) as u64;
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: rows as u64 * num_groups,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_gated_delta_rule_recurrence(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    q: &Buffer,
    q_offset: usize,
    k: &Buffer,
    k_offset: usize,
    v: &Buffer,
    v_offset: usize,
    g: &Buffer,
    g_offset: usize,
    beta: &Buffer,
    beta_offset: usize,
    state: &Buffer,
    state_offset: usize,
    out: &Buffer,
    out_offset: usize,
    bh: i32,
    seq_len: i32,
    k_dim: i32,
    v_dim: i32,
) -> Result<(), MetalKernelError> {
    let name = gdn_recurrence_kernel_name("gdn_gated_delta_rule_recurrence", ty, k_dim)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            (q, q_offset),
            (k, k_offset),
            (v, v_offset),
            (g, g_offset),
            (beta, beta_offset),
            (state, state_offset),
            (out, out_offset),
            bh,
            seq_len,
            v_dim
        )
    );

    let thread_group_size = MTLSize {
        width: 64,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((v_dim as u64) + 63) / 64,
        height: bh as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_gated_delta_rule_decode_slots(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    q: &Buffer,
    q_offset: usize,
    k: &Buffer,
    k_offset: usize,
    v: &Buffer,
    v_offset: usize,
    g: &Buffer,
    g_offset: usize,
    beta: &Buffer,
    beta_offset: usize,
    state: &Buffer,
    state_offset: usize,
    slots: &Buffer,
    slots_offset: usize,
    out: &Buffer,
    out_offset: usize,
    batch: i32,
    heads: i32,
    k_dim: i32,
    v_dim: i32,
) -> Result<(), MetalKernelError> {
    let name = gdn_recurrence_kernel_name("gdn_gated_delta_rule_decode_slots", ty, k_dim)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            (q, q_offset),
            (k, k_offset),
            (v, v_offset),
            (g, g_offset),
            (beta, beta_offset),
            (state, state_offset),
            (slots, slots_offset),
            (out, out_offset),
            batch,
            heads,
            k_dim,
            v_dim
        )
    );

    let thread_group_size = MTLSize {
        width: 64,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((v_dim as u64) + 63) / 64,
        height: (batch * heads) as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_gated_delta_rule_decode_slots_gqa(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    q: &Buffer,
    q_offset: usize,
    k: &Buffer,
    k_offset: usize,
    v: &Buffer,
    v_offset: usize,
    g: &Buffer,
    g_offset: usize,
    beta: &Buffer,
    beta_offset: usize,
    state: &Buffer,
    state_offset: usize,
    slots: &Buffer,
    slots_offset: usize,
    out: &Buffer,
    out_offset: usize,
    batch: i32,
    num_v_heads: i32,
    num_k_heads: i32,
    k_dim: i32,
    v_dim: i32,
    q_scale: f32,
) -> Result<(), MetalKernelError> {
    let name = gdn_recurrence_kernel_name("gdn_gated_delta_rule_decode_slots_gqa", ty, k_dim)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            (q, q_offset),
            (k, k_offset),
            (v, v_offset),
            (g, g_offset),
            (beta, beta_offset),
            (state, state_offset),
            (slots, slots_offset),
            (out, out_offset),
            batch,
            num_v_heads,
            num_k_heads,
            k_dim,
            v_dim,
            q_scale
        )
    );

    let thread_group_size = MTLSize {
        width: 64,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((v_dim as u64) + 63) / 64,
        height: (batch * num_v_heads) as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_gated_delta_rule_recurrence_varlen(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    q: &Buffer,
    q_offset: usize,
    k: &Buffer,
    k_offset: usize,
    v: &Buffer,
    v_offset: usize,
    g: &Buffer,
    g_offset: usize,
    beta: &Buffer,
    beta_offset: usize,
    state: &Buffer,
    state_offset: usize,
    slots: &Buffer,
    slots_offset: usize,
    out: &Buffer,
    out_offset: usize,
    state_snapshots: Option<(&Buffer, usize)>,
    cu_seqlens: &Buffer,
    cu_seqlens_offset: usize,
    batch: i32,
    num_heads: i32,
    k_dim: i32,
    v_dim: i32,
) -> Result<(), MetalKernelError> {
    let name = gdn_recurrence_kernel_name("gdn_gated_delta_rule_recurrence_varlen", ty, k_dim)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(1, Some(k), k_offset as NSUInteger);
    encoder.set_buffer(2, Some(v), v_offset as NSUInteger);
    encoder.set_buffer(3, Some(g), g_offset as NSUInteger);
    encoder.set_buffer(4, Some(beta), beta_offset as NSUInteger);
    encoder.set_buffer(5, Some(state), state_offset as NSUInteger);
    encoder.set_buffer(6, Some(slots), slots_offset as NSUInteger);
    encoder.set_buffer(7, Some(out), out_offset as NSUInteger);
    if let Some((snap, offset)) = state_snapshots {
        encoder.set_buffer(8, Some(snap), offset as NSUInteger);
    } else {
        encoder.set_buffer(8, None, 0);
    }
    encoder.set_buffer(9, Some(cu_seqlens), cu_seqlens_offset as NSUInteger);
    utils::set_param(encoder, 10, batch);
    utils::set_param(encoder, 11, num_heads);
    utils::set_param(encoder, 12, k_dim);
    utils::set_param(encoder, 13, v_dim);

    let thread_group_size = MTLSize {
        width: 64,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((v_dim as u64) + 63) / 64,
        height: (batch * num_heads) as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_gated_delta_rule_recurrence_varlen_gqa(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    q: &Buffer,
    q_offset: usize,
    k: &Buffer,
    k_offset: usize,
    v: &Buffer,
    v_offset: usize,
    g: &Buffer,
    g_offset: usize,
    beta: &Buffer,
    beta_offset: usize,
    state: &Buffer,
    state_offset: usize,
    slots: &Buffer,
    slots_offset: usize,
    out: &Buffer,
    out_offset: usize,
    state_snapshots: Option<(&Buffer, usize)>,
    cu_seqlens: &Buffer,
    cu_seqlens_offset: usize,
    batch: i32,
    num_v_heads: i32,
    num_k_heads: i32,
    k_dim: i32,
    v_dim: i32,
    q_scale: f32,
) -> Result<(), MetalKernelError> {
    let name = gdn_recurrence_kernel_name("gdn_gated_delta_rule_recurrence_varlen_gqa", ty, k_dim)?;
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(q), q_offset as NSUInteger);
    encoder.set_buffer(1, Some(k), k_offset as NSUInteger);
    encoder.set_buffer(2, Some(v), v_offset as NSUInteger);
    encoder.set_buffer(3, Some(g), g_offset as NSUInteger);
    encoder.set_buffer(4, Some(beta), beta_offset as NSUInteger);
    encoder.set_buffer(5, Some(state), state_offset as NSUInteger);
    encoder.set_buffer(6, Some(slots), slots_offset as NSUInteger);
    encoder.set_buffer(7, Some(out), out_offset as NSUInteger);
    if let Some((snap, offset)) = state_snapshots {
        encoder.set_buffer(8, Some(snap), offset as NSUInteger);
    } else {
        encoder.set_buffer(8, None, 0);
    }
    encoder.set_buffer(9, Some(cu_seqlens), cu_seqlens_offset as NSUInteger);
    utils::set_param(encoder, 10, batch);
    utils::set_param(encoder, 11, num_v_heads);
    utils::set_param(encoder, 12, num_k_heads);
    utils::set_param(encoder, 13, k_dim);
    utils::set_param(encoder, 14, v_dim);
    utils::set_param(encoder, 15, q_scale);

    let thread_group_size = MTLSize {
        width: 64,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: ((v_dim as u64) + 63) / 64,
        height: (batch * num_v_heads) as u64,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_gdn_mamba_scatter_rows(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    src: &Buffer,
    src_offset: usize,
    dst: &Buffer,
    dst_offset: usize,
    slots: &Buffer,
    slots_offset: usize,
    num_rows: i32,
    row_elems: i32,
    src_row_stride: i64,
    dst_row_stride: i64,
) -> Result<(), MetalKernelError> {
    let name = format!("gdn_mamba_scatter_rows_{}", gdn_type_name(ty)?);
    let pipeline = kernels.load_pipeline(device, name)?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            (src, src_offset),
            (dst, dst_offset),
            (slots, slots_offset),
            num_rows,
            row_elems,
            src_row_stride,
            dst_row_stride
        )
    );

    let total = (num_rows as u64) * (row_elems as u64);
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: (total + 255) / 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

// ---------------------------------------------------------------------------
// MLX NVFP4 utility kernels
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn call_mlx_nvfp4_repack(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    input: (&Buffer, usize),
    output: &Buffer,
    num_rows: usize,
    num_u32_cols: usize,
) -> Result<(), MetalKernelError> {
    let pipeline = kernels.load_pipeline(
        device,
        "mlx_nvfp4::mlx_nvfp4_repack_u32_to_u8_kernel".to_string(),
    )?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(input.0), input.1 as NSUInteger);
    encoder.set_buffer(1, Some(output), 0 as NSUInteger);
    utils::set_param(encoder, 2, num_rows as u32);
    utils::set_param(encoder, 3, num_u32_cols as u32);

    let total = (num_rows * num_u32_cols) as u64;
    let tg = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let gc = MTLSize {
        width: (total + 255) / 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(gc, tg);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_mlx_nvfp4_dequant_embedding(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    weight: (&Buffer, usize),
    scales: (&Buffer, usize),
    output: &Buffer,
    vocab_size: usize,
    hidden_size: usize,
) -> Result<(), MetalKernelError> {
    let name = match ty {
        DType::F16 => "mlx_nvfp4_dequant_embedding_f16",
        DType::BF16 => "mlx_nvfp4_dequant_embedding_bf16",
        other => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: other,
            })
        }
    };
    let pipeline = kernels.load_pipeline(device, name.to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    encoder.set_buffer(0, Some(weight.0), weight.1 as NSUInteger);
    encoder.set_buffer(1, Some(scales.0), scales.1 as NSUInteger);
    encoder.set_buffer(2, Some(output), 0 as NSUInteger);
    utils::set_param(encoder, 3, vocab_size as u32);
    utils::set_param(encoder, 4, hidden_size as u32);

    let total = (vocab_size * hidden_size / 2) as u64;
    let tg = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let gc = MTLSize {
        width: (total + 255) / 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(gc, tg);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_dflash_topk_select(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    scores: (&Buffer, usize),
    topk_weights: &Buffer,
    topk_indices: &Buffer,
    num_rows: u32,
    num_experts: u32,
    topk: u32,
) -> Result<(), MetalKernelError> {
    let pipeline = kernels.load_pipeline(device, "dflash_topk_select_float".to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            scores,
            topk_weights,
            topk_indices,
            num_rows,
            num_experts,
            topk
        )
    );

    let thread_groups_count = MTLSize {
        width: num_rows as u64,
        height: 1,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_dflash_select_candidates(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    hidden: (&Buffer, usize),
    unary_logits: (&Buffer, usize),
    candidate_ids: (&Buffer, usize),
    predecessor_codebook: (&Buffer, usize),
    successor_codebook: (&Buffer, usize),
    anchor_token: (&Buffer, usize),
    selected_tokens: &Buffer,
    sequence_len: u32,
    rank: u32,
    topk: u32,
) -> Result<(), MetalKernelError> {
    let pipeline = kernels.load_pipeline(device, "dflash_select_candidates_float".to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            hidden,
            unary_logits,
            candidate_ids,
            predecessor_codebook,
            successor_codebook,
            anchor_token,
            selected_tokens,
            sequence_len,
            rank,
            topk
        )
    );

    let thread_groups_count = MTLSize {
        width: 1,
        height: 1,
        depth: 1,
    };
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn call_dflash_grouped_conv(
    device: &Device,
    ep: impl EncoderProvider,
    kernels: &Kernels,
    ty: DType,
    hidden: (&Buffer, usize),
    delta: (&Buffer, usize),
    base_kernel: (&Buffer, usize),
    output: &Buffer,
    sequence_len: u32,
    hidden_size: u32,
    num_groups: u32,
    group_size: u32,
    taps: u32,
    block_size: u32,
    side: u32,
) -> Result<(), MetalKernelError> {
    let name = match ty {
        DType::F16 => "dflash_grouped_conv_half",
        DType::BF16 => "dflash_grouped_conv_bfloat16_t",
        other => {
            return Err(MetalKernelError::DTypeMismatch {
                expected: vec![DType::F16, DType::BF16],
                got: other,
            })
        }
    };
    let pipeline = kernels.load_pipeline(device, name.to_string())?;
    let encoder = ep.encoder();
    let encoder: &ComputeCommandEncoderRef = encoder.as_ref();
    encoder.set_compute_pipeline_state(&pipeline);
    set_params!(
        encoder,
        (
            hidden,
            delta,
            base_kernel,
            output,
            sequence_len,
            hidden_size,
            num_groups,
            group_size,
            taps,
            block_size,
            side
        )
    );

    let total = sequence_len as u64 * hidden_size as u64;
    let thread_group_size = MTLSize {
        width: 256,
        height: 1,
        depth: 1,
    };
    let thread_groups_count = MTLSize {
        width: (total + 255) / 256,
        height: 1,
        depth: 1,
    };
    encoder.dispatch_thread_groups(thread_groups_count, thread_group_size);
    Ok(())
}
