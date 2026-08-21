// ============================================================
// 朋友圈图片解密模块 — ISAAC-64 密码核
// 自 sns_image.rs 拆分：微信 WxIsaac64 兼容密钥流生成。
// ============================================================

/// ISAAC-64 黄金比例常数（微信 WxIsaac64 使用的版本）
const GOLDEN: u64 = 0x9e3779b97f4a7c13;

// ============ ISAAC-64 ============

/// ISAAC-64 伪随机数生成器（WeChat WxIsaac64 兼容实现）
pub(crate) struct Isaac64 {
    mm: [u64; 256],
    randrsl: [u64; 256],
    aa: u64,
    bb: u64,
    cc: u64,
    randcnt: usize,
}

impl Isaac64 {
    /// 以 seed 初始化（randrsl[0]=seed，其余清零），并完成一轮预生成
    pub(crate) fn new(seed: u64) -> Self {
        let mut s = Self {
            mm: [0u64; 256],
            randrsl: [0u64; 256],
            aa: 0,
            bb: 0,
            cc: 0,
            randcnt: 255,
        };
        s.randrsl[0] = seed;

        let mut a = GOLDEN;
        let mut b = GOLDEN;
        let mut c = GOLDEN;
        let mut d = GOLDEN;
        let mut e = GOLDEN;
        let mut f = GOLDEN;
        let mut g = GOLDEN;
        let mut h = GOLDEN;

        for _ in 0..4 {
            mix(
                &mut a, &mut b, &mut c, &mut d, &mut e, &mut f, &mut g, &mut h,
            );
        }

        let mut i = 0;
        while i < 256 {
            a = a.wrapping_add(s.randrsl[i]);
            b = b.wrapping_add(s.randrsl[i + 1]);
            c = c.wrapping_add(s.randrsl[i + 2]);
            d = d.wrapping_add(s.randrsl[i + 3]);
            e = e.wrapping_add(s.randrsl[i + 4]);
            f = f.wrapping_add(s.randrsl[i + 5]);
            g = g.wrapping_add(s.randrsl[i + 6]);
            h = h.wrapping_add(s.randrsl[i + 7]);
            mix(
                &mut a, &mut b, &mut c, &mut d, &mut e, &mut f, &mut g, &mut h,
            );
            s.mm[i] = a;
            s.mm[i + 1] = b;
            s.mm[i + 2] = c;
            s.mm[i + 3] = d;
            s.mm[i + 4] = e;
            s.mm[i + 5] = f;
            s.mm[i + 6] = g;
            s.mm[i + 7] = h;
            i += 8;
        }

        let mut i = 0;
        while i < 256 {
            a = a.wrapping_add(s.mm[i]);
            b = b.wrapping_add(s.mm[i + 1]);
            c = c.wrapping_add(s.mm[i + 2]);
            d = d.wrapping_add(s.mm[i + 3]);
            e = e.wrapping_add(s.mm[i + 4]);
            f = f.wrapping_add(s.mm[i + 5]);
            g = g.wrapping_add(s.mm[i + 6]);
            h = h.wrapping_add(s.mm[i + 7]);
            mix(
                &mut a, &mut b, &mut c, &mut d, &mut e, &mut f, &mut g, &mut h,
            );
            s.mm[i] = a;
            s.mm[i + 1] = b;
            s.mm[i + 2] = c;
            s.mm[i + 3] = d;
            s.mm[i + 4] = e;
            s.mm[i + 5] = f;
            s.mm[i + 6] = g;
            s.mm[i + 7] = h;
            i += 8;
        }

        s.isaac64();
        s
    }

    /// 一轮 ISAAC-64 生成（填充 randrsl[0..256]）
    fn isaac64(&mut self) {
        self.cc = self.cc.wrapping_add(1);
        self.bb = self.bb.wrapping_add(self.cc);
        for i in 0..256 {
            match i & 3 {
                0 => self.aa = !(self.aa ^ self.aa.wrapping_shl(21)),
                1 => self.aa ^= self.aa >> 5,
                2 => self.aa ^= self.aa.wrapping_shl(12),
                _ => self.aa ^= self.aa >> 33,
            }
            self.aa = self.aa.wrapping_add(self.mm[(i + 128) & 255]);
            let x = self.mm[i];
            let y = self.mm[((x >> 3) & 255) as usize]
                .wrapping_add(self.aa)
                .wrapping_add(self.bb);
            self.mm[i] = y;
            self.bb = self.mm[((y >> 11) & 255) as usize].wrapping_add(x);
            self.randrsl[i] = self.bb;
        }
    }

    /// 取下一个 64 位随机数（按 randrsl 降序，与微信实现一致）
    fn next_u64(&mut self) -> u64 {
        let v = self.randrsl[self.randcnt];
        if self.randcnt == 0 {
            self.isaac64();
            self.randcnt = 255;
        } else {
            self.randcnt -= 1;
        }
        v
    }

    /// 生成长度为 size 的密钥流（每 8 字节一个大端 64 位随机数）
    pub(crate) fn keystream(&mut self, size: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(size);
        while out.len() < size {
            out.extend_from_slice(&self.next_u64().to_be_bytes());
        }
        out.truncate(size);
        out
    }
}

/// ISAAC-64 mix 函数
// 8 个状态字为算法内联状态，拆分进结构体会降低可读性，保留扁平参数
#[allow(clippy::too_many_arguments)]
fn mix(
    a: &mut u64,
    b: &mut u64,
    c: &mut u64,
    d: &mut u64,
    e: &mut u64,
    f: &mut u64,
    g: &mut u64,
    h: &mut u64,
) {
    *a = a.wrapping_sub(*e);
    *f ^= *h >> 9;
    *h = h.wrapping_add(*a);
    *b = b.wrapping_sub(*f);
    *g ^= a.wrapping_shl(9);
    *a = a.wrapping_add(*b);
    *c = c.wrapping_sub(*g);
    *h ^= *b >> 23;
    *b = b.wrapping_add(*c);
    *d = d.wrapping_sub(*h);
    *a ^= c.wrapping_shl(15);
    *c = c.wrapping_add(*d);
    *e = e.wrapping_sub(*a);
    *b ^= *d >> 14;
    *d = d.wrapping_add(*e);
    *f = f.wrapping_sub(*b);
    *c ^= e.wrapping_shl(20);
    *e = e.wrapping_add(*f);
    *g = g.wrapping_sub(*c);
    *d ^= *f >> 17;
    *f = f.wrapping_add(*g);
    *h = h.wrapping_sub(*d);
    *e ^= g.wrapping_shl(14);
    *g = g.wrapping_add(*h);
}
