//! Statistics of a buffer of pixels, computed on the GPU: one XCL-owned compute kernel (never a
//! title pipeline) writes a 32-word `Partial` per thread group (per-channel min / max / sum,
//! counts of all-zero, NaN and infinite elements, two order-independent hashes) and the CPU
//! combines the partials into `Stats` (`combine`). Used by the pass snapshots
//! (`hle/d3d12x/pass_capture.rs`): copy a render target or UAV into a buffer, run `record` on it
//! in an XCL list, read the partials back.
//!
//! Element decoding by `FormatClass`: channels are reported in R, G, B, A order whatever the
//! memory order (BGRA is swapped); `D24UnormS8` gives depth in [0, 1] and the stencil value;
//! `R32Uint` and `Raw32` give the word converted to float and never count as NaN or Inf. An
//! element with a NaN channel counts in `nan`; one with an Inf channel and no NaN counts in `inf`;
//! both are left out of min / max / sum, so `finite = count - nan - inf`.

use std::sync::{Mutex, OnceLock};

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::recomp::dxc;

/// How the kernel decodes an element. The discriminant is the `class` root constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatClass {
    Rgba8Unorm = 0,
    Bgra8Unorm = 1,
    Rgba16Float = 2,
    Rg16Float = 3,
    R16Float = 4,
    Rgba32Float = 5,
    Rg32Float = 6,
    R32Float = 7,
    R11G11B10Float = 8,
    Rgb10A2Unorm = 9,
    D32Float = 10,
    D24UnormS8 = 11,
    R32Uint = 12,
    Raw32 = 13,
}

/// The class of a DXGI format and its bytes per element. Typeless formats map to their
/// float / unorm sibling; anything else (block-compressed, 16-bit integer, ...) is `Raw32`, read
/// as 4-byte words.
pub fn class_of(fmt: DXGI_FORMAT) -> (FormatClass, u32) {
    use FormatClass::*;
    match fmt {
        DXGI_FORMAT_R8G8B8A8_TYPELESS | DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => (Rgba8Unorm, 4),
        DXGI_FORMAT_B8G8R8A8_TYPELESS | DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8X8_TYPELESS | DXGI_FORMAT_B8G8R8X8_UNORM | DXGI_FORMAT_B8G8R8X8_UNORM_SRGB => (Bgra8Unorm, 4),
        DXGI_FORMAT_R16G16B16A16_TYPELESS | DXGI_FORMAT_R16G16B16A16_FLOAT => (Rgba16Float, 8),
        DXGI_FORMAT_R16G16_TYPELESS | DXGI_FORMAT_R16G16_FLOAT => (Rg16Float, 4),
        DXGI_FORMAT_R16_TYPELESS | DXGI_FORMAT_R16_FLOAT => (R16Float, 2),
        DXGI_FORMAT_R32G32B32A32_TYPELESS | DXGI_FORMAT_R32G32B32A32_FLOAT => (Rgba32Float, 16),
        DXGI_FORMAT_R32G32_TYPELESS | DXGI_FORMAT_R32G32_FLOAT => (Rg32Float, 8),
        DXGI_FORMAT_R32_TYPELESS | DXGI_FORMAT_R32_FLOAT => (R32Float, 4),
        DXGI_FORMAT_R11G11B10_FLOAT => (R11G11B10Float, 4),
        DXGI_FORMAT_R10G10B10A2_TYPELESS | DXGI_FORMAT_R10G10B10A2_UNORM => (Rgb10A2Unorm, 4),
        DXGI_FORMAT_D32_FLOAT => (D32Float, 4),
        DXGI_FORMAT_D24_UNORM_S8_UINT | DXGI_FORMAT_R24G8_TYPELESS | DXGI_FORMAT_R24_UNORM_X8_TYPELESS => (D24UnormS8, 4),
        DXGI_FORMAT_R32_UINT => (R32Uint, 4),
        _ => (Raw32, 4),
    }
}

/// Words in one thread group's partial.
pub const PARTIAL_WORDS: usize = 32;
/// Elements one thread group covers (256 threads x 16 elements).
pub const GROUP_ELEMENTS: u32 = 256 * 16;
/// Thread groups one dispatch can have (`D3D12_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION`);
/// larger buffers are split by the caller with `base_offset` and `elements`.
pub const MAX_GROUPS: u32 = 65_535;

/// The root constants: `elements` to read, laid out linearly (`row_elems` = 0) or in rows of
/// `row_elems` elements `row_pitch` bytes apart, `elem_bytes` each, starting `base_offset` bytes
/// into the source. `base_offset` and `row_pitch` must be multiples of 4 (of 2 for `R16Float`),
/// `elem_bytes` the one `class_of` gave.
#[derive(Clone, Copy, Debug)]
pub struct Consts {
    pub elements: u32,
    pub row_elems: u32,
    pub row_pitch: u32,
    pub elem_bytes: u32,
    pub class: FormatClass,
    pub base_offset: u32,
}

/// Thread groups `record` dispatches for `elements` (the partials buffer needs
/// `groups_for(elements) * PARTIAL_WORDS * 4` bytes).
pub fn groups_for(elements: u32) -> u32 {
    elements.div_ceil(GROUP_ELEMENTS).min(MAX_GROUPS)
}

/// One thread group's result. Word layout of the 32 words the kernel writes (`from_words`):
/// 0..4 `min` (as f32 bits), 4..8 `max`, 8..12 `sum`, 12 `zero`, 13 `nan`, 14 `inf`, 15 `count`,
/// 16 `h1`, 17 `h2`, 18..32 reserved (zero). `min` / `max` / `sum` cover the finite elements
/// only; a group with none has `min` = +Inf and `max` = -Inf in the channels the class has;
/// channels the class does not have are 0. `zero` counts elements whose channels are all 0.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Partial {
    pub min: [f32; 4],
    pub max: [f32; 4],
    pub sum: [f32; 4],
    pub zero: u32,
    pub nan: u32,
    pub inf: u32,
    pub count: u32,
    pub h1: u32,
    pub h2: u32,
}

impl Partial {
    /// From the first 18 words of a partial (see the layout above).
    pub fn from_words(w: &[u32]) -> Partial {
        let f = |at: usize| [0, 1, 2, 3].map(|k| f32::from_bits(w[at + k]));
        Partial { min: f(0), max: f(4), sum: f(8), zero: w[12], nan: w[13], inf: w[14], count: w[15], h1: w[16], h2: w[17] }
    }
}

/// The result over all groups. `min`, `max` and `mean` are over the `finite` elements (0 when
/// there are none). `hash` is `(sum of h1) << 32 | (sum of h2)`, each sum wrapping in 32 bits:
/// the same for the same words wherever they sit, whatever the order of elements and groups.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub min: [f32; 4],
    pub max: [f32; 4],
    pub mean: [f32; 4],
    pub zero: u64,
    pub nan: u64,
    pub inf: u64,
    pub count: u64,
    pub finite: u64,
    pub hash: u64,
}

/// Merge the partials of one dispatch. Pure, CPU-side.
pub fn combine(parts: &[Partial]) -> Stats {
    let mut s = Stats { min: [f32::INFINITY; 4], max: [f32::NEG_INFINITY; 4], ..Default::default() };
    let (mut sum, mut h1, mut h2) = ([0f64; 4], 0u32, 0u32);
    for p in parts {
        for k in 0..4 {
            s.min[k] = s.min[k].min(p.min[k]);
            s.max[k] = s.max[k].max(p.max[k]);
            sum[k] += f64::from(p.sum[k]);
        }
        s.zero += u64::from(p.zero);
        s.nan += u64::from(p.nan);
        s.inf += u64::from(p.inf);
        s.count += u64::from(p.count);
        h1 = h1.wrapping_add(p.h1);
        h2 = h2.wrapping_add(p.h2);
    }
    s.finite = s.count.saturating_sub(s.nan + s.inf);
    if s.finite == 0 {
        s.min = [0.0; 4];
        s.max = [0.0; 4];
    } else {
        s.mean = sum.map(|v| (v / s.finite as f64) as f32);
    }
    s.hash = (u64::from(h1) << 32) | u64::from(h2);
    s
}

/// The kernel. `[numthreads(256,1,1)]`, one group per `GROUP_ELEMENTS` elements, each thread
/// reads 16 elements (`k * 256 + tid` apart, so a wave reads neighbours). Root parameters: 0 = six
/// root constants (b0: `Consts`), 1 = root SRV t0 `src`, 2 = root UAV u0 `partials`. Hashes, over
/// the raw 32-bit words of each element (the 16-bit value for `R16Float`), word `j` of element `i`
/// having index `i * 4 + j`: `h1 += mix1(word ^ index * 0x9E3779B9)`, `h2 += mix2(word + index)`;
/// `mix1` is the murmur3 finaliser, `mix2` is the "lowbias32" mixer. The sums wrap.
const HLSL: &str = r#"
cbuffer C : register(b0) {
    uint elements;
    uint row_elems;
    uint row_pitch;
    uint elem_bytes;
    uint cls;
    uint base_offset;
};
ByteAddressBuffer src : register(t0);
RWByteAddressBuffer partials : register(u0);

// 0 min.xyzw, 4 max.xyzw, 8 sum.xyzw, 12 zero, 13 nan, 14 inf, 15 count, 16 h1, 17 h2; 64 waves
groupshared uint gs[18 * 64];

uint mix1(uint h) {
    h ^= h >> 16; h *= 0x85ebca6bu; h ^= h >> 13; h *= 0xc2b2ae35u; h ^= h >> 16;
    return h;
}
uint mix2(uint x) {
    x ^= x >> 16; x *= 0x7feb352du; x ^= x >> 15; x *= 0x846ca68bu; x ^= x >> 16;
    return x;
}
float h16(uint v) { return f16tof32(v & 0xFFFFu); }

uint chans_of(uint c) {
    if (c == 0 || c == 1 || c == 2 || c == 5 || c == 9) return 4;
    if (c == 8) return 3;
    if (c == 3 || c == 6 || c == 11) return 2;
    return 1;
}
uint words_of(uint c) {
    if (c == 2 || c == 6) return 2;
    if (c == 5) return 4;
    return 1;
}

void decode(uint off, out float4 c, out uint4 w) {
    c = 0; w = 0;
    if (cls == 0 || cls == 1) {
        uint v = src.Load(off); w.x = v;
        c = float4(v & 255u, (v >> 8) & 255u, (v >> 16) & 255u, v >> 24) / 255.0;
        if (cls == 1) c = c.zyxw;
    } else if (cls == 2) {
        uint2 v = src.Load2(off); w.xy = v;
        c = float4(h16(v.x), h16(v.x >> 16), h16(v.y), h16(v.y >> 16));
    } else if (cls == 3) {
        uint v = src.Load(off); w.x = v;
        c.xy = float2(h16(v), h16(v >> 16));
    } else if (cls == 4) {
        uint v = src.Load(off & ~3u);
        v = ((off & 2u) != 0) ? (v >> 16) : (v & 0xFFFFu);
        w.x = v; c.x = h16(v);
    } else if (cls == 5) {
        w = src.Load4(off); c = asfloat(w);
    } else if (cls == 6) {
        uint2 v = src.Load2(off); w.xy = v; c.xy = asfloat(v);
    } else if (cls == 7 || cls == 10) {
        uint v = src.Load(off); w.x = v; c.x = asfloat(v);
    } else if (cls == 8) {
        uint v = src.Load(off); w.x = v;
        c.xyz = float3(f16tof32((v & 0x7FFu) << 4), f16tof32(((v >> 11) & 0x7FFu) << 4), f16tof32(((v >> 22) & 0x3FFu) << 5));
    } else if (cls == 9) {
        uint v = src.Load(off); w.x = v;
        c = float4(v & 1023u, (v >> 10) & 1023u, (v >> 20) & 1023u, 0) / 1023.0;
        c.w = (v >> 30) / 3.0;
    } else if (cls == 11) {
        uint v = src.Load(off); w.x = v;
        c.x = (v & 0xFFFFFFu) / 16777215.0; c.y = v >> 24;
    } else {
        uint v = src.Load(off); w.x = v; c.x = v;
    }
}

bool is_nan(float x) { uint b = asuint(x); return (b & 0x7F800000u) == 0x7F800000u && (b & 0x7FFFFFu) != 0; }
bool is_inf(float x) { return (asuint(x) & 0x7FFFFFFFu) == 0x7F800000u; }

[numthreads(256, 1, 1)]
void main(uint tid : SV_GroupIndex, uint3 gid : SV_GroupID) {
    const uint nch = chans_of(cls);
    const uint nw = words_of(cls);
    const bool floaty = (cls <= 8 && cls != 0 && cls != 1) || cls == 10 || cls == 5;
    float4 mn = asfloat(0x7F800000u), mx = asfloat(0xFF800000u), sm = 0;
    uint zero = 0, nan = 0, inf = 0, count = 0, h1 = 0, h2 = 0;
    for (uint k = 0; k < 16; k++) {
        uint idx = gid.x * 4096 + k * 256 + tid;
        if (idx >= elements) break;
        uint off = base_offset + ((row_elems == 0) ? idx * elem_bytes : (idx / row_elems) * row_pitch + (idx % row_elems) * elem_bytes);
        float4 c; uint4 w;
        decode(off, c, w);
        for (uint j = 0; j < nw; j++) {
            uint i = idx * 4 + j;
            h1 += mix1(w[j] ^ (i * 0x9E3779B9u));
            h2 += mix2(w[j] + i);
        }
        count++;
        bool bn = false, bi = false, bz = true;
        for (uint ch = 0; ch < nch; ch++) {
            if (floaty) { bn = bn || is_nan(c[ch]); bi = bi || is_inf(c[ch]); }
            bz = bz && (c[ch] == 0.0);
        }
        if (bn) { nan++; continue; }
        if (bi) { inf++; continue; }
        if (bz) zero++;
        mn = min(mn, c); mx = max(mx, c); sm += c;
    }
    float4 wmn = WaveActiveMin(mn), wmx = WaveActiveMax(mx), wsm = WaveActiveSum(sm);
    uint wz = WaveActiveSum(zero), wn = WaveActiveSum(nan), wi = WaveActiveSum(inf);
    uint wc = WaveActiveSum(count), wh1 = WaveActiveSum(h1), wh2 = WaveActiveSum(h2);
    const uint lanes = WaveGetLaneCount();
    if (WaveIsFirstLane()) {
        uint s = tid / lanes;
        [unroll] for (uint q = 0; q < 4; q++) {
            gs[q * 64 + s] = asuint(wmn[q]);
            gs[(4 + q) * 64 + s] = asuint(wmx[q]);
            gs[(8 + q) * 64 + s] = asuint(wsm[q]);
        }
        gs[12 * 64 + s] = wz; gs[13 * 64 + s] = wn; gs[14 * 64 + s] = wi;
        gs[15 * 64 + s] = wc; gs[16 * 64 + s] = wh1; gs[17 * 64 + s] = wh2;
    }
    GroupMemoryBarrierWithGroupSync();
    if (tid != 0) return;
    float4 gmn = asfloat(0x7F800000u), gmx = asfloat(0xFF800000u), gsm = 0;
    uint gz = 0, gn = 0, gi = 0, gc = 0, g1 = 0, g2 = 0;
    for (uint s = 0; s < 256 / lanes; s++) {
        [unroll] for (uint q = 0; q < 4; q++) {
            gmn[q] = min(gmn[q], asfloat(gs[q * 64 + s]));
            gmx[q] = max(gmx[q], asfloat(gs[(4 + q) * 64 + s]));
            gsm[q] += asfloat(gs[(8 + q) * 64 + s]);
        }
        gz += gs[12 * 64 + s]; gn += gs[13 * 64 + s]; gi += gs[14 * 64 + s];
        gc += gs[15 * 64 + s]; g1 += gs[16 * 64 + s]; g2 += gs[17 * 64 + s];
    }
    [unroll] for (uint q = 0; q < 4; q++) {
        if (q >= nch) { gmn[q] = 0; gmx[q] = 0; gsm[q] = 0; }
    }
    uint at = gid.x * 128;
    partials.Store4(at, asuint(gmn));
    partials.Store4(at + 16, asuint(gmx));
    partials.Store4(at + 32, asuint(gsm));
    partials.Store4(at + 48, uint4(gz, gn, gi, gc));
    partials.Store4(at + 64, uint4(g1, g2, 0, 0));
    partials.Store4(at + 80, uint4(0, 0, 0, 0));
    partials.Store4(at + 96, uint4(0, 0, 0, 0));
    partials.Store4(at + 112, uint4(0, 0, 0, 0));
}
"#;

/// The kernel's root signature (0: six root constants b0, 1: root SRV t0, 2: root UAV u0) and
/// pipeline. XCL-owned.
pub struct Kernel {
    rs: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}

// D3D12 objects are free-threaded.
unsafe impl Send for Kernel {}
unsafe impl Sync for Kernel {}

fn build(dev: &ID3D12Device) -> Result<Kernel, String> {
    static DXIL: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
    let dxil = DXIL.get_or_init(|| dxc::compile(HLSL, "main", "cs_6_0", &[])).as_ref().map_err(|x| format!("pass stats kernel: {x}"))?;
    unsafe {
        let root = |ty: D3D12_ROOT_PARAMETER_TYPE, anon: D3D12_ROOT_PARAMETER1_0| D3D12_ROOT_PARAMETER1 {
            ParameterType: ty,
            Anonymous: anon,
            ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
        };
        let view = |reg: u32| D3D12_ROOT_PARAMETER1_0 {
            Descriptor: D3D12_ROOT_DESCRIPTOR1 { ShaderRegister: reg, RegisterSpace: 0, Flags: D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE },
        };
        let params = [
            root(D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS, D3D12_ROOT_PARAMETER1_0 {
                Constants: D3D12_ROOT_CONSTANTS { ShaderRegister: 0, RegisterSpace: 0, Num32BitValues: 6 },
            }),
            root(D3D12_ROOT_PARAMETER_TYPE_SRV, view(0)),
            root(D3D12_ROOT_PARAMETER_TYPE_UAV, view(0)),
        ];
        let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
            Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
            Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
                Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                    NumParameters: params.len() as u32,
                    pParameters: params.as_ptr(),
                    NumStaticSamplers: 0,
                    pStaticSamplers: std::ptr::null(),
                    Flags: D3D12_ROOT_SIGNATURE_FLAG_NONE,
                },
            },
        };
        let mut blob: Option<ID3DBlob> = None;
        D3D12SerializeVersionedRootSignature(&desc, &mut blob, None).map_err(|e| format!("pass stats root signature: {e}"))?;
        let blob = blob.ok_or("pass stats root signature")?;
        let rs: ID3D12RootSignature = dev
            .CreateRootSignature(0, std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize()))
            .map_err(|e| format!("pass stats CreateRootSignature: {e}"))?;
        let pso: ID3D12PipelineState = dev
            .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                pRootSignature: std::mem::transmute_copy(&rs),
                CS: D3D12_SHADER_BYTECODE { pShaderBytecode: dxil.as_ptr() as _, BytecodeLength: dxil.len() },
                ..Default::default()
            })
            .map_err(|e| format!("pass stats CreateComputePipelineState: {e}"))?;
        Ok(Kernel { rs, pso })
    }
}

/// The kernel for `dev`, built on first use (the shader is compiled once for the process). A
/// built kernel lives for the rest of the process and keeps its device alive; the runtime has one.
pub fn kernel(dev: &ID3D12Device) -> Result<&'static Kernel, String> {
    static BUILT: Mutex<Vec<(ID3D12Device, &'static Kernel)>> = Mutex::new(Vec::new());
    let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, k)) = built.iter().find(|(d, _)| d.as_raw() == dev.as_raw()) {
        return Ok(k);
    }
    let k: &'static Kernel = Box::leak(Box::new(build(dev)?));
    built.push((dev.clone(), k));
    Ok(k)
}

/// Record the statistics dispatch of `c` over the buffer at `src_va` into `list`, writing
/// `groups * PARTIAL_WORDS * 4` bytes at `partials_va`; returns the groups (`groups_for`). Sets
/// the compute root signature, the pipeline and the three root arguments: use an XCL list, never
/// a title's. The caller transitions `src` to a shader-readable state and `partials` to
/// UNORDERED_ACCESS first, and the source must hold the elements `c` names.
pub fn record(list: &ID3D12GraphicsCommandList, k: &Kernel, src_va: u64, partials_va: u64, c: Consts) -> u32 {
    let groups = groups_for(c.elements);
    if groups == 0 {
        return 0;
    }
    let words = [c.elements, c.row_elems, c.row_pitch, c.elem_bytes, c.class as u32, c.base_offset];
    unsafe {
        list.SetComputeRootSignature(&k.rs);
        list.SetPipelineState(&k.pso);
        list.SetComputeRoot32BitConstants(0, words.len() as u32, words.as_ptr().cast(), 0);
        list.SetComputeRootShaderResourceView(1, src_va);
        list.SetComputeRootUnorderedAccessView(2, partials_va);
        list.Dispatch(groups, 1, 1);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    fn part(min: f32, max: f32, sum: f32, zero: u32, nan: u32, count: u32, h1: u32, h2: u32) -> Partial {
        Partial { min: [min; 4], max: [max; 4], sum: [sum; 4], zero, nan, inf: 0, count, h1, h2 }
    }

    #[test]
    fn class_of_maps_common_targets() {
        assert_eq!(class_of(DXGI_FORMAT_R16G16B16A16_FLOAT), (FormatClass::Rgba16Float, 8));
        assert_eq!(class_of(DXGI_FORMAT_R16G16B16A16_TYPELESS), (FormatClass::Rgba16Float, 8));
        assert_eq!(class_of(DXGI_FORMAT_R11G11B10_FLOAT), (FormatClass::R11G11B10Float, 4));
        assert_eq!(class_of(DXGI_FORMAT_D32_FLOAT), (FormatClass::D32Float, 4));
        assert_eq!(class_of(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB), (FormatClass::Bgra8Unorm, 4));
        assert_eq!(class_of(DXGI_FORMAT_R32G32B32A32_FLOAT), (FormatClass::Rgba32Float, 16));
        assert_eq!(class_of(DXGI_FORMAT_BC7_UNORM), (FormatClass::Raw32, 4));
    }

    #[test]
    fn combine_merges_min_max_mean_and_counts() {
        let s = combine(&[part(0.25, 1.0, 10.0, 2, 1, 10, 1, 2), part(-1.0, 0.5, 6.0, 3, 0, 10, 3, 4)]);
        assert_eq!(s.min, [-1.0; 4]);
        assert_eq!(s.max, [1.0; 4]);
        assert_eq!((s.zero, s.nan, s.inf, s.count, s.finite), (5, 1, 0, 20, 19));
        assert!((s.mean[0] - 16.0 / 19.0).abs() < 1e-6);
        assert_eq!(s.hash, (4u64 << 32) | 6);
        // no finite element at all: min / max / mean are 0, not the +-Inf of an empty group
        let e = combine(&[Partial { min: [f32::INFINITY; 4], max: [f32::NEG_INFINITY; 4], nan: 4, count: 4, ..Default::default() }]);
        assert_eq!((e.finite, e.min, e.max, e.mean), (0, [0.0; 4], [0.0; 4], [0.0; 4]));
    }

    #[test]
    fn combine_hash_is_order_independent() {
        let parts = [part(0.0, 1.0, 1.0, 0, 0, 1, 0xFFFF_FFF0, 0x8000_0001), part(0.0, 1.0, 1.0, 0, 0, 1, 0x20, 0x8000_0002), part(0.0, 1.0, 1.0, 0, 0, 1, 7, 0xFFFF_FFFF)];
        let mut rev = parts;
        rev.reverse();
        let (a, b) = (combine(&parts), combine(&rev));
        assert_eq!(a.hash, b.hash);
        // the sums wrap: 0xFFFFFFF0 + 0x20 + 7 = 0x17 mod 2^32
        assert_eq!((a.hash >> 32) as u32, 0x17);
    }

    fn transition(r: &ID3D12Resource, before: D3D12_RESOURCE_STATES, after: D3D12_RESOURCE_STATES) -> D3D12_RESOURCE_BARRIER {
        D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: unsafe { std::mem::transmute_copy(r) },
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: before,
                    StateAfter: after,
                }),
            },
        }
    }

    unsafe fn buf(dev: &ID3D12Device, size: u64, heap: D3D12_HEAP_TYPE, state: D3D12_RESOURCE_STATES, uav: bool) -> ID3D12Resource {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: size,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: if uav { D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS } else { D3D12_RESOURCE_FLAG_NONE },
            ..Default::default()
        };
        let mut r: Option<ID3D12Resource> = None;
        dev.CreateCommittedResource(&D3D12_HEAP_PROPERTIES { Type: heap, ..Default::default() }, D3D12_HEAP_FLAG_NONE, &desc, state, None, &mut r).unwrap();
        r.unwrap()
    }

    /// Run the kernel over `data` (read from an upload buffer) and combine the partials.
    unsafe fn run(dev: &ID3D12Device, queue: &ID3D12CommandQueue, k: &Kernel, data: &[u8], c: Consts) -> Stats {
        let src = buf(dev, data.len() as u64, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ, false);
        let mut p = std::ptr::null_mut();
        src.Map(0, None, Some(&mut p)).unwrap();
        std::ptr::copy_nonoverlapping(data.as_ptr(), p as *mut u8, data.len());
        src.Unmap(0, None);
        let bytes = (groups_for(c.elements) as usize * PARTIAL_WORDS * 4) as u64;
        let partials = buf(dev, bytes, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COMMON, true);
        let rb = buf(dev, bytes, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST, false);
        let alloc: ID3D12CommandAllocator = dev.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
        let list: ID3D12GraphicsCommandList = dev.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None).unwrap();
        list.ResourceBarrier(&[transition(&partials, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_UNORDERED_ACCESS)]);
        let groups = record(&list, k, src.GetGPUVirtualAddress(), partials.GetGPUVirtualAddress(), c);
        assert_eq!(groups, groups_for(c.elements));
        list.ResourceBarrier(&[transition(&partials, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
        list.CopyBufferRegion(&rb, 0, &partials, 0, bytes);
        list.Close().unwrap();
        queue.ExecuteCommandLists(&[Some(list.cast().unwrap())]);
        crate::gpu::texture::wait(dev, queue).unwrap();
        let mut q = std::ptr::null_mut();
        rb.Map(0, None, Some(&mut q)).unwrap();
        let words = std::slice::from_raw_parts(q as *const u32, bytes as usize / 4).to_vec();
        rb.Unmap(0, None);
        let parts: Vec<Partial> = words.chunks(PARTIAL_WORDS).map(Partial::from_words).collect();
        combine(&parts)
    }

    fn mix1(mut h: u32) -> u32 {
        h ^= h >> 16;
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 13;
        h = h.wrapping_mul(0xc2b2_ae35);
        h ^ (h >> 16)
    }

    fn mix2(mut x: u32) -> u32 {
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846c_a68b);
        x ^ (x >> 16)
    }

    /// The hash the kernel must give for `data` read as 8-byte elements (two words each).
    fn hash_rgba16(data: &[u8]) -> u64 {
        let (mut h1, mut h2) = (0u32, 0u32);
        for (i, el) in data.chunks(8).enumerate() {
            for j in 0..2 {
                let w = u32::from_le_bytes(el[4 * j..4 * j + 4].try_into().unwrap());
                let idx = (i * 4 + j) as u32;
                h1 = h1.wrapping_add(mix1(w ^ idx.wrapping_mul(0x9E37_79B9)));
                h2 = h2.wrapping_add(mix2(w.wrapping_add(idx)));
            }
        }
        (u64::from(h1) << 32) | u64::from(h2)
    }

    #[test]
    fn gpu_kernel_reports_nan_zero_and_constant() {
        let (dev, queue) = unsafe {
            let dev = crate::gpu::agility::create_device(None, windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0).ok();
            match dev.and_then(|d| d.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC { Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default() }).ok().map(|q| (d, q))) {
                Some(x) => x,
                None => {
                    crate::gpu::recomp::runner::skip_test("no GPU");
                    return;
                }
            }
        };
        if dxc::location().is_err() {
            crate::gpu::recomp::runner::skip_test("no DXC");
            return;
        }
        let k = match kernel(&dev) {
            Ok(k) => k,
            Err(x) => panic!("{x}"),
        };
        // 64 x 64 RGBA16F texels: 3 NaN (all channels), 100 zero, the rest 0.5
        let (half, nan_half) = (0x3800u16, 0x7E00u16);
        let mut data = Vec::new();
        for i in 0..64 * 64 {
            let v = if (5..8).contains(&i) { nan_half } else if (100..200).contains(&i) { 0 } else { half };
            for _ in 0..4 {
                data.extend_from_slice(&v.to_le_bytes());
            }
        }
        let c = Consts { elements: 64 * 64, row_elems: 64, row_pitch: 64 * 8, elem_bytes: 8, class: FormatClass::Rgba16Float, base_offset: 0 };
        let s = unsafe { run(&dev, &queue, k, &data, c) };
        assert_eq!((s.nan, s.zero, s.inf, s.count, s.finite), (3, 100, 0, 4096, 4093), "{s:?}");
        assert_eq!(s.min, [0.0; 4], "{s:?}");
        assert_eq!(s.max, [0.5; 4], "{s:?}");
        assert!((s.mean[0] - 1996.5 / 4093.0).abs() < 1e-4, "{s:?}");
        assert_eq!(s.hash, hash_rgba16(&data), "{s:?}");
        // one texel changed: the hash differs (and so does the maximum)
        let mut changed = data.clone();
        changed[4000 * 8..4000 * 8 + 2].copy_from_slice(&0x3C00u16.to_le_bytes());
        let t = unsafe { run(&dev, &queue, k, &changed, c) };
        assert_ne!(t.hash, s.hash);
        assert_eq!(t.max[0], 1.0, "{t:?}");
        assert_eq!(t.hash, hash_rgba16(&changed), "{t:?}");
    }
}
