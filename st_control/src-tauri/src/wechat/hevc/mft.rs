// ============================================================
// wxgf / HEVC 图片转码 — Media Foundation 解码域
// 自 hevc.rs 拆分：MFT 枚举 / 媒体类型协商 / NV12 输出提取。
// ============================================================

#![allow(non_snake_case)]

use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::*;

use super::{nv12_to_rgb, parse_sps_dimensions};

/// Media Foundation HEVC 解码，输出第一帧 RGB24
pub(crate) unsafe fn decode_hevc_to_rgb(bitstream: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    // COM 初始化（已初始化则忽略错误）
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    MFStartup(MF_VERSION, MFSTARTUP_FULL).ok()?;

    let result = decode_inner(bitstream);

    MFShutdown().ok();
    result
}

unsafe fn decode_inner(bitstream: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    // ── 1. 枚举 HEVC 同步解码器 MFT ──
    let input_reg = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_HEVC,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count: u32 = 0;
    // 先试 HEVC 子类型，再试 H265（部分解码器只注册其一）
    let mut found = MFTEnumEx(
        MFT_CATEGORY_VIDEO_DECODER,
        MFT_ENUM_FLAG_SYNCMFT,
        Some(&input_reg),
        None,
        &mut activates,
        &mut count,
    );
    if found.is_err() || count == 0 {
        let input_reg2 = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H265,
        };
        found = MFTEnumEx(
            MFT_CATEGORY_VIDEO_DECODER,
            MFT_ENUM_FLAG_SYNCMFT,
            Some(&input_reg2),
            None,
            &mut activates,
            &mut count,
        );
    }
    if found.is_err() || count == 0 || activates.is_null() {
        eprintln!(
            "[hevc-dbg] 无解码器 MFT err={:?} count={}",
            found.err(),
            count
        );
        return None;
    }

    // 激活第一个解码器。
    // 必须先克隆 COM 智能指针（AddRef）再释放枚举数组，
    // 否则激活后使用的是已释放内存（曾导致 0xC0000005 访问违例）
    let activate: IMFActivate = (*activates).clone()?;
    let mft: IMFTransform = activate.ActivateObject().ok()?;
    CoTaskMemFree(Some(activates as *const _));

    // ── 2. 设置输入/输出媒体类型 ──
    let in_type: IMFMediaType = MFCreateMediaType().ok()?;
    in_type
        .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
        .ok()?;
    in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_HEVC).ok()?;
    // 解码器要求输入类型携带 MF_MT_FRAME_SIZE（缺则后续 SetOutputType
    // 报 MF_E_ATTRIBUTENOTFOUND）：从 SPS 解析宽高补齐
    if let Some((w, h)) = parse_sps_dimensions(bitstream) {
        let _ = in_type.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64);
    }
    if mft.SetInputType(0, &in_type, 0).is_err() {
        // 部分解码器只认 H265 子类型
        in_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H265).ok()?;
        mft.SetInputType(0, &in_type, 0).ok()?;
    }

    // 该解码器（HEVCVideoExtension）要求 ProcessInput 前必须设置输出类型，
    // 但直接 SetOutputType 会因类型缺少属性报 MF_E_ATTRIBUTENOTFOUND。
    // 做法：取其可用输出类型并补齐常见属性
    let mut out_set = false;
    for i in 0..8 {
        let t = match mft.GetOutputAvailableType(0, i) {
            Ok(t) => t,
            Err(_) => break,
        };
        let sub = t.GetGUID(&MF_MT_SUBTYPE).unwrap_or_default();
        if sub != MFVideoFormat_NV12 {
            continue;
        }
        if mft.SetOutputType(0, &t, 0).is_ok() {
            out_set = true;
            break;
        }
        // 补齐属性后重试：渐进隔行、1:1 像素比、独立样本
        let _ = t.SetUINT32(&MF_MT_INTERLACE_MODE, 2); // Progressive
        let _ = t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1);
        let _ = t.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1);
        if mft.SetOutputType(0, &t, 0).is_ok() {
            out_set = true;
            break;
        }
    }
    if !out_set {
        log::debug!("[hevc] 无法设置解码器输出类型");
        return None;
    }

    // ── 3. 流控制消息 ──
    // 该解码器对部分 ProcessMessage 消息处理有缺陷（实测会 0xC0000005），
    // NOTIFY_* 均为可选消息，全部跳过，直接 ProcessInput 喂流

    // ── 4. 整段码流作为单个 sample 送入 ──
    let sample: IMFSample = MFCreateSample().ok()?;
    let buffer: IMFMediaBuffer = MFCreateMemoryBuffer(bitstream.len() as u32).ok()?;
    {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut max_len: u32 = 0;
        buffer.Lock(&mut ptr, Some(&mut max_len), None).ok()?;
        std::ptr::copy_nonoverlapping(bitstream.as_ptr(), ptr, bitstream.len());
        buffer.Unlock().ok()?;
        buffer.SetCurrentLength(bitstream.len() as u32).ok()?;
    }
    sample.AddBuffer(&buffer).ok()?;
    sample.SetSampleTime(0).ok()?;
    sample.SetSampleDuration(1).ok()?;

    // ── 5. 送输入（必要时先排空输出再重试）──
    let mut input_done = false;
    let mut frames: Vec<(Vec<u8>, u32, u32, u32)> = Vec::new(); // (nv12, w, h, stride)
                                                                // 解码器向调用方 1D 缓冲写入时的行距 = 编码宽度（SPS 原始宽，未裁剪）
    let coded_stride = parse_sps_dimensions(bitstream).map(|(w, _)| w).unwrap_or(0);

    for _ in 0..16 {
        if !input_done {
            match mft.ProcessInput(0, &sample, 0) {
                Ok(()) => {
                    input_done = true;
                    mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0).ok();
                    mft.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0).ok();
                }
                Err(e) if e.code() == MF_E_NOTACCEPTING => {
                    drain_output(&mft, &mut frames, coded_stride);
                    continue;
                }
                Err(e) => {
                    log::debug!("[hevc] ProcessInput 失败 {:?}", e);
                    return None;
                }
            }
        }
        match try_output(&mft, &mut frames, coded_stride) {
            OutputState::Frame | OutputState::StreamChange => {
                if !frames.is_empty() {
                    break; // 拿到第一帧即可
                }
            }
            OutputState::NeedMoreInput => {
                if input_done {
                    break;
                }
            }
            OutputState::Fatal => return None,
        }
    }

    let _ = mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);

    let (nv12, w, h, stride) = frames.into_iter().next()?;
    nv12_to_rgb(&nv12, w as usize, h as usize, stride as usize).map(|rgb| (rgb, w, h))
}

enum OutputState {
    Frame,
    StreamChange,
    NeedMoreInput,
    Fatal,
}

/// 尝试取一帧输出；成功时把 NV12 数据存入 frames
unsafe fn try_output(
    mft: &IMFTransform,
    frames: &mut Vec<(Vec<u8>, u32, u32, u32)>,
    coded_stride: u32,
) -> OutputState {
    // 该解码器要求调用方提供输出 sample（pSample=None 会报 E_INVALIDARG）
    let out_sample = match MFCreateSample() {
        Ok(s) => s,
        Err(_) => return OutputState::Fatal,
    };
    let out_buffer = match MFCreateMemoryBuffer(32 * 1024 * 1024) {
        Ok(b) => b,
        Err(_) => return OutputState::Fatal,
    };
    if out_sample.AddBuffer(&out_buffer).is_err() {
        return OutputState::Fatal;
    }
    let mut out_buf = MFT_OUTPUT_DATA_BUFFER {
        dwStreamID: 0,
        pSample: std::mem::ManuallyDrop::new(Some(out_sample)),
        dwStatus: 0,
        pEvents: std::mem::ManuallyDrop::new(None),
    };
    let mut status: u32 = 0;
    match mft.ProcessOutput(0, std::slice::from_mut(&mut out_buf), &mut status) {
        Ok(()) => {
            if let Some(sample) = out_buf.pSample.as_ref() {
                if let Some(frame) = extract_nv12(mft, sample, coded_stride) {
                    frames.push(frame);
                    return OutputState::Frame;
                }
            }
            OutputState::StreamChange
        }
        Err(e) => {
            if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
                OutputState::NeedMoreInput
            } else if e.code() == MF_E_TRANSFORM_STREAM_CHANGE {
                OutputState::StreamChange
            } else {
                log::debug!("[hevc] ProcessOutput 失败 {:?}", e);
                OutputState::Fatal
            }
        }
    }
}

fn drain_output(mft: &IMFTransform, frames: &mut Vec<(Vec<u8>, u32, u32, u32)>, coded_stride: u32) {
    for _ in 0..8 {
        match unsafe { try_output(mft, frames, coded_stride) } {
            OutputState::Frame | OutputState::StreamChange => continue,
            _ => break,
        }
    }
}

/// 从输出 sample 提取 NV12 数据与帧参数
///
/// `coded_stride`：SPS 解析的编码宽度。调用方 1D 缓冲下解码器按编码
/// 宽度紧凑写入（裁剪只是显示提示），FRAME_SIZE/DEFAULT_STRIDE 属性
/// 给的却是裁剪后显示宽度，用错会出现斜向撕裂。
unsafe fn extract_nv12(
    mft: &IMFTransform,
    sample: &IMFSample,
    coded_stride: u32,
) -> Option<(Vec<u8>, u32, u32, u32)> {
    let cur_type = mft.GetOutputCurrentType(0).ok()?;
    // 只接受 NV12 输出（解码器默认值）；其他格式不处理
    let subtype = cur_type.GetGUID(&MF_MT_SUBTYPE).ok()?;
    if subtype != MFVideoFormat_NV12 {
        log::debug!("[hevc] 输出格式非 NV12: {:?}", subtype);
        return None;
    }
    let wh: u64 = cur_type.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
    let width = (wh >> 32) as u32;
    let height = (wh & 0xFFFF_FFFF) as u32;
    let stride = if coded_stride >= width && coded_stride > 0 {
        coded_stride
    } else {
        cur_type.GetUINT32(&MF_MT_DEFAULT_STRIDE).unwrap_or(width)
    };

    let buf = sample.ConvertToContiguousBuffer().ok()?;
    let mut ptr: *mut u8 = std::ptr::null_mut();
    let mut cur_len: u32 = 0;
    buf.Lock(&mut ptr, None, Some(&mut cur_len)).ok()?;
    let need = stride as usize * height as usize * 3 / 2;
    if (cur_len as usize) < need {
        log::debug!("[hevc] 缓冲长度不足: {} < {}", cur_len, need);
        buf.Unlock().ok();
        return None;
    }
    let data = std::slice::from_raw_parts(ptr, need).to_vec();
    buf.Unlock().ok()?;

    if width == 0 || height == 0 || data.is_empty() {
        return None;
    }
    Some((data, width, height, stride))
}
