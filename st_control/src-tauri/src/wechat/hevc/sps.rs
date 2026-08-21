// ============================================================
// wxgf / HEVC 图片转码 — SPS 位流解析域
// 自 hevc.rs 拆分：极简 H.265 SPS 解析器（宽高提取）。
// ============================================================

// ── 极简 HEVC SPS 解析器：提取图像宽高（解码器要求输入类型带 FRAME_SIZE）──

pub(crate) struct BitReader<'a> {
    data: &'a [u8],
    pos: usize, // bit 位置
}

impl<'a> BitReader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub(crate) fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.pos / 8)?;
        let b = (byte >> (7 - (self.pos % 8))) & 1;
        self.pos += 1;
        Some(b as u32)
    }
    pub(crate) fn bits(&mut self, n: usize) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Some(v)
    }
    /// unsigned Exp-Golomb
    pub(crate) fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0usize;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        if zeros == 0 {
            return Some(0);
        }
        Some(((1u64 << zeros) - 1 + self.bits(zeros)? as u64) as u32)
    }
}

/// 在 Annex-B 码流中定位 SPS NAL 并解析 (宽, 高)
pub(crate) fn parse_sps_dimensions(stream: &[u8]) -> Option<(u32, u32)> {
    // 找 00 00 00 01 / 00 00 01 起始码，逐个 NAL 检查类型
    let mut i = 0usize;
    while i + 5 < stream.len() {
        let (start_len, nal_off) = if stream[i..].starts_with(&[0, 0, 0, 1]) {
            (4usize, i + 4)
        } else if stream[i..].starts_with(&[0, 0, 1]) {
            (3usize, i + 3)
        } else {
            i += 1;
            continue;
        };
        if nal_off + 2 > stream.len() {
            break;
        }
        let nal_type = (stream[nal_off] >> 1) & 0x3F;
        if nal_type == 33 {
            // SPS：NAL 头 2 字节后是去 emulation-prevention 的 RBSP
            let mut rbsp = Vec::with_capacity(256);
            let mut j = nal_off + 2;
            let mut zeros = 0;
            while j < stream.len() && rbsp.len() < 4096 {
                let b = stream[j];
                if zeros == 2 && b == 3 {
                    zeros = 0;
                    j += 1;
                    continue;
                }
                if zeros >= 2 && b <= 1 {
                    break; // 下一个起始码
                }
                if b == 0 {
                    zeros += 1;
                } else {
                    zeros = 0;
                }
                rbsp.push(b);
                j += 1;
            }
            if let Some(dims) = parse_sps_rbsp(&rbsp) {
                return Some(dims);
            }
        }
        i += start_len.max(1);
    }
    None
}

/// 解析 SPS RBSP 中的 pic_width/pic_height（含 conformance window 修正）
fn parse_sps_rbsp(rbsp: &[u8]) -> Option<(u32, u32)> {
    let mut r = BitReader::new(rbsp);
    r.bits(4)?; // sps_video_parameter_set_id
    let max_sub_layers_minus1 = r.bits(3)? as usize;
    r.bit()?; // sps_temporal_id_nesting_flag
              // profile_tier_level（共 96 bit）
    r.bits(2 + 1 + 5)?; // profile_space, tier_flag, profile_idc
    r.bits(32)?; // profile_compatibility_flags
    r.bits(4)?; // progressive/interlaced/non_packed/frame_only
    r.bits(44)?; // general_reserved_zero_44bits
    r.bits(8)?; // general_level_idc
    if max_sub_layers_minus1 > 0 {
        let mut prof_pres = [0u8; 8];
        let mut lvl_pres = [0u8; 8];
        for i in 0..max_sub_layers_minus1 {
            prof_pres[i] = r.bit()? as u8;
            lvl_pres[i] = r.bit()? as u8;
        }
        if max_sub_layers_minus1 < 8 {
            for _ in max_sub_layers_minus1..8 {
                r.bits(2)?;
            }
        }
        for i in 0..max_sub_layers_minus1 {
            if prof_pres[i] == 1 {
                r.bits(2 + 1 + 5 + 32 + 4 + 44 + 8)?; // 与 general 相同的 96 bit
            }
            if lvl_pres[i] == 1 {
                r.bits(8)?;
            }
        }
    }
    r.ue()?; // sps_seq_parameter_set_id
    let chroma_format_idc = r.ue()?;
    if chroma_format_idc == 3 {
        r.bit()?; // separate_colour_plane_flag
    }
    let width = r.ue()?;
    let height = r.ue()?;
    // 返回【编码宽度】（不做 conformance 裁剪）：解码缓冲按编码尺寸布局，
    // 行距必须用编码宽度；裁剪只影响显示提示
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return None;
    }
    Some((width, height))
}
