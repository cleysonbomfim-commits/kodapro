// KODA Pro grade nodes (koda-color), run by filmcraft-gpu's effect stage as external passes.
// Bindings and uniform layout are fx.wgsl's: the working image arrives in `src` (linear
// premultiplied RGBA f32), the result goes to `dst`, and `aux` holds a node's extra data (a 3D
// LUT unfolded into 2D: x = r + g·n, y = b). One pass = one node: u.i1.x is the node type,
// u.i1.yzw and u.p0..p5 its parameters. The math mirrors koda_color::eval / koda_color::cst.

struct U {
    i0: vec4<u32>,  // op, width, height, unused
    i1: vec4<u32>,  // node type, integer parameters
    p0: vec4<f32>,
    p1: vec4<f32>,
    p2: vec4<f32>,
    p3: vec4<f32>,
    p4: vec4<f32>,
    p5: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<rgba32float, write>;
@group(0) @binding(3) var aux: texture_2d<f32>;

const NODE_GAIN: u32 = 1u;
const NODE_CST: u32 = 2u;
const NODE_LUT: u32 = 3u;
const NODE_PRIMARIES: u32 = 4u;
const NODE_MID_DETAIL: u32 = 5u;
const NODE_TO_SIGNAL: u32 = 6u;
const NODE_GAUSS: u32 = 7u;
const NODE_SHARPEN: u32 = 8u;
const NODE_GLOW_DOWN: u32 = 9u;
const NODE_GLOW_BLUR: u32 = 11u;
const NODE_GLOW: u32 = 10u;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

const LN10: f32 = 2.302585092994046;
const SLOG3_BREAK: f32 = 171.2102946929;

// Linear light -> grading signal (extended sRGB, odd-symmetric, no clipping).
fn to_signal1(x: f32) -> f32 {
    let a = abs(x);
    var s: f32;
    if a <= 0.0031308 {
        s = a * 12.92;
    } else {
        s = 1.055 * pow(a, 1.0 / 2.4) - 0.055;
    }
    return select(s, -s, x < 0.0);
}

fn to_linear1(x: f32) -> f32 {
    let a = abs(x);
    var l: f32;
    if a <= 0.04045 {
        l = a / 12.92;
    } else {
        l = pow((a + 0.055) / 1.055, 2.4);
    }
    return select(l, -l, x < 0.0);
}

// Odd-symmetric power.
fn spow(x: f32, e: f32) -> f32 {
    let p = pow(abs(x), e);
    return select(p, -p, x < 0.0);
}

// CST curves (koda_color::cst::Curve codes): 1 S-Log3, 2 gamma 2.4, 3 gamma 2.2, 4 sRGB, 5 linear.
fn curve_decode(v: f32, c: u32) -> f32 {
    switch c {
        case 1u: {
            if v >= SLOG3_BREAK / 1023.0 {
                return exp(((v * 1023.0 - 420.0) / 261.5) * LN10) * 0.19 - 0.01;
            }
            return (v * 1023.0 - 95.0) * 0.01125 / (SLOG3_BREAK - 95.0);
        }
        case 2u: { return spow(v, 2.4); }
        case 3u: { return spow(v, 2.2); }
        case 4u: { return to_linear1(v); }
        default: { return v; }
    }
}

fn curve_encode(l: f32, c: u32) -> f32 {
    switch c {
        case 1u: {
            if l >= 0.01125 {
                return (420.0 + log((l + 0.01) / 0.19) / LN10 * 261.5) / 1023.0;
            }
            return (l * (SLOG3_BREAK - 95.0) / 0.01125 + 95.0) / 1023.0;
        }
        case 2u: { return spow(l, 1.0 / 2.4); }
        case 3u: { return spow(l, 1.0 / 2.2); }
        case 4u: { return to_signal1(l); }
        default: { return l; }
    }
}

// The KODA filmic curve, one channel (koda_color::cst::filmic).
fn filmic(v: f32, white: f32, mid: f32, contrast: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let code = white / (1.0 + exp(-(log2(v) - mid) / contrast));
    return pow(max(code, 0.0), 2.4);
}

// Gamut compression (koda_color::cst::compress).
fn compress(c: vec3<f32>) -> vec3<f32> {
    let th = 0.8;
    let lim = 1.25;
    let p = 1.2;
    let ach = max(c.r, max(c.g, c.b));
    if ach <= 1e-9 {
        return c;
    }
    let s = (lim - th) / pow(pow((lim - th) / (1.0 - th), p) - 1.0, 1.0 / p);
    var o = c;
    for (var i = 0; i < 3; i++) {
        let d = (ach - c[i]) / ach;
        if d >= th {
            let x = (d - th) / s;
            let dc = th + s * x / pow(1.0 + pow(x, p), 1.0 / p);
            o[i] = ach - dc * ach;
        }
    }
    return o;
}

fn cst(c: vec3<f32>) -> vec3<f32> {
    let l = vec3<f32>(curve_decode(c.r, u.i1.y), curve_decode(c.g, u.i1.y), curve_decode(c.b, u.i1.y));
    // row-major matrix in p0.xyz, p0.w p1.xy, p1.zw p2.x
    var o = vec3<f32>(
        u.p0.x * l.r + u.p0.y * l.g + u.p0.z * l.b,
        u.p0.w * l.r + u.p1.x * l.g + u.p1.y * l.b,
        u.p1.z * l.r + u.p1.w * l.g + u.p2.x * l.b,
    );
    if (u.i1.w & 1u) != 0u {
        let white = u.p3.x;
        let mid = u.p3.y;
        let contrast = u.p3.z;
        o = vec3<f32>(filmic(o.r, white, mid, contrast), filmic(o.g, white, mid, contrast), filmic(o.b, white, mid, contrast));
    }
    if (u.i1.w & 2u) != 0u {
        o = compress(o);
    }
    return vec3<f32>(curve_encode(o.r, u.i1.z), curve_encode(o.g, u.i1.z), curve_encode(o.b, u.i1.z));
}

// LUT grid point (r, g, b).
fn lut_at(r: i32, g: i32, b: i32, n: i32) -> vec3<f32> {
    return textureLoad(aux, vec2<i32>(r + g * n, b), 0).rgb;
}

// LUT node (koda_color::eval: filmcraft_color::Lut3d::apply_trilinear / apply, mixed by intensity).
fn lut(c: vec3<f32>) -> vec3<f32> {
    let n = i32(u.i1.z);
    let dmin = u.p1.xyz;
    let dmax = u.p2.xyz;
    let x = clamp((c - dmin) / (dmax - dmin), vec3<f32>(0.0), vec3<f32>(1.0)) * f32(n - 1);
    let i0 = min(vec3<i32>(x), vec3<i32>(n - 2));
    let f = x - vec3<f32>(i0);
    let r0 = i0.x;
    let g0 = i0.y;
    let b0 = i0.z;
    var v: vec3<f32>;
    if u.i1.y == 0u {
        let x00 = mix(lut_at(r0, g0, b0, n), lut_at(r0 + 1, g0, b0, n), f.x);
        let x10 = mix(lut_at(r0, g0 + 1, b0, n), lut_at(r0 + 1, g0 + 1, b0, n), f.x);
        let x01 = mix(lut_at(r0, g0, b0 + 1, n), lut_at(r0 + 1, g0, b0 + 1, n), f.x);
        let x11 = mix(lut_at(r0, g0 + 1, b0 + 1, n), lut_at(r0 + 1, g0 + 1, b0 + 1, n), f.x);
        v = mix(mix(x00, x10, f.y), mix(x01, x11, f.y), f.z);
    } else {
        let c000 = lut_at(r0, g0, b0, n);
        let c111 = lut_at(r0 + 1, g0 + 1, b0 + 1, n);
        var ca: vec3<f32>;
        var cb: vec3<f32>;
        var w: vec4<f32>;
        let fr = f.x;
        let fg = f.y;
        let fb = f.z;
        if fr > fg {
            if fg > fb {
                ca = lut_at(r0 + 1, g0, b0, n); cb = lut_at(r0 + 1, g0 + 1, b0, n); w = vec4<f32>(1.0 - fr, fr - fg, fg - fb, fb);
            } else if fr > fb {
                ca = lut_at(r0 + 1, g0, b0, n); cb = lut_at(r0 + 1, g0, b0 + 1, n); w = vec4<f32>(1.0 - fr, fr - fb, fb - fg, fg);
            } else {
                ca = lut_at(r0, g0, b0 + 1, n); cb = lut_at(r0 + 1, g0, b0 + 1, n); w = vec4<f32>(1.0 - fb, fb - fr, fr - fg, fg);
            }
        } else if fb > fg {
            ca = lut_at(r0, g0, b0 + 1, n); cb = lut_at(r0, g0 + 1, b0 + 1, n); w = vec4<f32>(1.0 - fb, fb - fg, fg - fr, fr);
        } else if fb > fr {
            ca = lut_at(r0, g0 + 1, b0, n); cb = lut_at(r0, g0 + 1, b0 + 1, n); w = vec4<f32>(1.0 - fg, fg - fb, fb - fr, fr);
        } else {
            ca = lut_at(r0, g0 + 1, b0, n); cb = lut_at(r0 + 1, g0 + 1, b0, n); w = vec4<f32>(1.0 - fg, fg - fr, fr - fb, fb);
        }
        v = w.x * c000 + w.y * ca + w.z * cb + w.w * c111;
    }
    return c + (v - c) * u.p0.x;
}

// Primaries node parameter `i` (koda_color::primaries::Ready::p, row 0 of aux, 4 per texel).
fn pp(i: i32) -> f32 {
    let t = textureLoad(aux, vec2<i32>(i / 4, 0), 0);
    return t[i % 4];
}

// Curve `c` (0 = Y, 1..3 = R, G, B) at x: the baked table (row 1), linear in between, extended
// past [0, 1] with the end slopes.
fn pcurve(c: i32, x: f32) -> f32 {
    let n = i32(u.i1.y);
    if x <= 0.0 {
        return textureLoad(aux, vec2<i32>(0, 1), 0)[c] + pp(28 + c) * x;
    }
    if x >= 1.0 {
        return textureLoad(aux, vec2<i32>(n - 1, 1), 0)[c] + pp(32 + c) * (x - 1.0);
    }
    let f = x * f32(n - 1);
    let i = min(i32(f), n - 2);
    let t = f - f32(i);
    let a = textureLoad(aux, vec2<i32>(i, 1), 0)[c];
    let b = textureLoad(aux, vec2<i32>(i + 1, 1), 0)[c];
    return a + (b - a) * t;
}

fn lgg(x: f32, k: i32) -> f32 {
    return spow((x + pp(4 + k) * (1.0 - x)) * pp(8 + k), pp(12 + k));
}

fn smoothw(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Primaries and curves (koda_color::primaries::Ready::apply).
fn prim(c0: vec3<f32>) -> vec3<f32> {
    var c = c0 * vec3<f32>(pp(0), pp(1), pp(2));
    c = vec3<f32>(lgg(c.r, 1), lgg(c.g, 2), lgg(c.b, 3));
    let y0 = dot(c, LUMA);
    let dy = lgg(y0, 0) - y0;
    let m = pp(3);
    c = vec3<f32>(lgg(c.r, 0), lgg(c.g, 0), lgg(c.b, 0)) * (1.0 - m) + (c + vec3<f32>(dy)) * m;
    c = c + vec3<f32>(pp(16), pp(17), pp(18));
    c = (c - vec3<f32>(pp(20))) * pp(19) + vec3<f32>(pp(20));
    let y1 = dot(c, LUMA);
    c = c + vec3<f32>(pp(21) * (1.0 - smoothw(0.0, 0.6, y1)) + pp(22) * smoothw(0.4, 1.0, y1));
    let y = dot(c, LUMA);
    let cb0 = (c.b - y) / 1.8556;
    let cr0 = (c.r - y) / 1.5748;
    let chroma = sqrt(cb0 * cb0 + cr0 * cr0);
    let k = pp(24) * (1.0 + pp(23) * (1.0 - min(chroma * 4.0, 1.0)));
    let cb = (cb0 * pp(25) - cr0 * pp(26)) * k;
    let cr = (cb0 * pp(26) + cr0 * pp(25)) * k;
    let r = y + 1.5748 * cr;
    let b = y + 1.8556 * cb;
    let g = (y - LUMA.r * r - LUMA.b * b) / LUMA.g;
    c = vec3<f32>(r, g, b);
    if pp(27) > 0.5 {
        c = vec3<f32>(pcurve(1, pcurve(0, c.r)), pcurve(2, pcurve(0, c.g)), pcurve(3, pcurve(0, c.b)));
    }
    return c;
}

fn node(c: vec3<f32>) -> vec3<f32> {
    switch u.i1.x {
        case NODE_GAIN: {
            return c * u.p0.xyz;
        }
        case NODE_CST: {
            return cst(c);
        }
        case NODE_LUT: {
            return lut(c);
        }
        case NODE_PRIMARIES: {
            return prim(c);
        }
        default: {
            return c;
        }
    }
}

fn unpremul_signal(px: vec4<f32>) -> vec3<f32> {
    return vec3<f32>(to_signal1(px.r / px.a), to_signal1(px.g / px.a), to_signal1(px.b / px.a));
}

// Mid Detail combine (koda_color::primaries::mid_detail): `src` holds the Gaussian, `aux` the
// image before it.
fn mid_detail(p: vec2<i32>) -> vec4<f32> {
    let o = textureLoad(aux, p, 0);
    let bl = textureLoad(src, p, 0);
    if o.a <= 0.0 || bl.a <= 0.0 {
        return o;
    }
    let oc = unpremul_signal(o);
    let yo = dot(oc, LUMA);
    let yb = dot(unpremul_signal(bl), LUMA);
    let w = clamp(4.0 * yo * (1.0 - yo), 0.0, 1.0);
    let d = (yo - yb) * u.p0.x * 1.5 * w;
    let c = oc + vec3<f32>(d);
    return vec4<f32>(to_linear1(c.r) * o.a, to_linear1(c.g) * o.a, to_linear1(c.b) * o.a, o.a);
}

// One direction of a separable Gaussian (koda_color::eval::gauss_cpu): sigma u.p0.x, taps each
// side u.i1.y, vertical when u.i1.z == 1, edges clamped, all four channels.
fn gauss_pass(p: vec2<i32>) -> vec4<f32> {
    let n = i32(u.i1.y);
    let sigma = u.p0.x;
    let size = vec2<i32>(i32(u.i0.y), i32(u.i0.z));
    var acc = vec4<f32>(0.0);
    var total = 0.0;
    for (var j = -n; j <= n; j++) {
        total += exp(-f32(j * j) / (2.0 * sigma * sigma));
    }
    for (var j = -n; j <= n; j++) {
        let w = exp(-f32(j * j) / (2.0 * sigma * sigma)) / max(total, 1e-12);
        var q = p;
        if u.i1.z == 1u {
            q.y = clamp(p.y + j, 0, size.y - 1);
        } else {
            q.x = clamp(p.x + j, 0, size.x - 1);
        }
        acc += textureLoad(src, q, 0) * w;
    }
    return acc;
}

fn unpremul(px: vec4<f32>) -> vec3<f32> {
    if px.a <= 0.0 {
        return vec3<f32>(0.0);
    }
    return px.rgb / px.a;
}

// Sharpen combine: `src` the blurred signal, `aux` the signal (both premultiplied).
fn sharpen(p: vec2<i32>) -> vec4<f32> {
    let s = textureLoad(aux, p, 0);
    if s.a <= 0.0 {
        return s;
    }
    let x = unpremul(s);
    let g = unpremul(textureLoad(src, p, 0));
    let o = x + u.p0.x * (x - g);
    return vec4<f32>(to_linear1(o.r) * s.a, to_linear1(o.g) * s.a, to_linear1(o.b) * s.a, s.a);
}

// Glow source mask (koda_color::spatial::Glow::mask): threshold u.p0.x.
fn glow_mask(c: vec3<f32>) -> f32 {
    let t = u.p0.x;
    if t <= 0.0 {
        return 1.0;
    }
    return smoothw(t - 0.15, t + 0.15, dot(c, LUMA));
}

// The glow works on a copy reduced k = u.i1.z times, kept in the top-left corner of the
// texture (koda_color::eval::glow_down): rw × rh = ceil(size / k).
fn glow_region() -> vec2<i32> {
    let k = i32(max(u.i1.z, 1u));
    return vec2<i32>((i32(u.i0.y) + k - 1) / k, (i32(u.i0.z) + k - 1) / k);
}

// The masked source, premultiplied signal, as k × k block means (`src` = the picture).
fn glow_down(p: vec2<i32>) -> vec4<f32> {
    let reg = glow_region();
    if p.x >= reg.x || p.y >= reg.y {
        return vec4<f32>(0.0);
    }
    let k = i32(max(u.i1.z, 1u));
    let size = vec2<i32>(i32(u.i0.y), i32(u.i0.z));
    var acc = vec4<f32>(0.0);
    var n = 0.0;
    for (var yy = p.y * k; yy < min((p.y + 1) * k, size.y); yy++) {
        for (var xx = p.x * k; xx < min((p.x + 1) * k, size.x); xx++) {
            let q = textureLoad(src, vec2<i32>(xx, yy), 0);
            n += 1.0;
            if q.a > 0.0 {
                let s = unpremul_signal(q);
                acc += vec4<f32>(s * glow_mask(s) * q.a, q.a);
            }
        }
    }
    return acc / max(n, 1.0);
}

// One direction of the glow's Gaussian inside the reduced copy (sigma u.p2.x, taps u.i1.y,
// vertical when u.i1.w == 1), edges clamped to the copy.
fn glow_blur(p: vec2<i32>) -> vec4<f32> {
    let reg = glow_region();
    if p.x >= reg.x || p.y >= reg.y {
        return vec4<f32>(0.0);
    }
    let n = i32(u.i1.y);
    let sigma = u.p2.x;
    var total = 0.0;
    for (var j = -n; j <= n; j++) {
        total += exp(-f32(j * j) / (2.0 * sigma * sigma));
    }
    var acc = vec4<f32>(0.0);
    for (var j = -n; j <= n; j++) {
        let w = exp(-f32(j * j) / (2.0 * sigma * sigma)) / max(total, 1e-12);
        var q = p;
        if u.i1.w == 1u {
            q.y = clamp(p.y + j, 0, reg.y - 1);
        } else {
            q.x = clamp(p.x + j, 0, reg.x - 1);
        }
        acc += textureLoad(src, q, 0) * w;
    }
    return acc;
}

// Bilinear sample of the reduced copy at pixel p of the picture.
fn glow_up(p: vec2<i32>) -> vec4<f32> {
    let reg = glow_region();
    let k = f32(max(u.i1.z, 1u));
    let f = clamp((vec2<f32>(p) + vec2<f32>(0.5)) / k - vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(reg - vec2<i32>(1)));
    let i0 = vec2<i32>(floor(f));
    let i1 = min(i0 + vec2<i32>(1), reg - vec2<i32>(1));
    let t = f - vec2<f32>(i0);
    let a = textureLoad(src, i0, 0);
    let b = textureLoad(src, vec2<i32>(i1.x, i0.y), 0);
    let c = textureLoad(src, vec2<i32>(i0.x, i1.y), 0);
    let d = textureLoad(src, i1, 0);
    return (a * (1.0 - t.x) + b * t.x) * (1.0 - t.y) + (c * (1.0 - t.x) + d * t.x) * t.y;
}

fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    }
    var d: f32;
    if b <= 0.25 {
        d = ((16.0 * b - 12.0) * b + 4.0) * b;
    } else {
        d = sqrt(max(b, 0.0));
    }
    return b + (2.0 * s - 1.0) * (d - b);
}

fn recover(v: f32, amount: f32) -> f32 {
    let knee = 0.8;
    if amount <= 0.0 || v <= knee {
        return v;
    }
    let r = knee + (1.0 - knee) * (1.0 - exp(-(v - knee) / (1.0 - knee)));
    return v + (r - v) * amount;
}

// Glow combine (koda_color::spatial::Glow::composite): `src` the blurred source, `aux` the
// picture. p0 = threshold, gain, saturation, gamma; p1 = core, opacity, recovery; i1.y style.
fn glow(p: vec2<i32>) -> vec4<f32> {
    let o = textureLoad(aux, p, 0);
    if o.a <= 0.0 {
        return o;
    }
    let x = unpremul_signal(o);
    let srcp = x * glow_mask(x);
    let k = u.p1.x;
    let g0 = unpremul(glow_up(p)) * (1.0 - k) + srcp * k;
    let y = dot(g0, LUMA);
    let g = vec3<f32>(
        pow(max(y + u.p0.z * (g0.r - y), 0.0), u.p0.w),
        pow(max(y + u.p0.z * (g0.g - y), 0.0), u.p0.w),
        pow(max(y + u.p0.z * (g0.b - y), 0.0), u.p0.w),
    ) * u.p0.y;
    var c: vec3<f32>;
    for (var i = 0; i < 3; i++) {
        let b = x[i];
        let t = g[i];
        var v: f32;
        switch u.i1.y {
            case 0u: { v = soft_light(t, b); }
            case 1u: { v = 1.0 - (1.0 - b) * (1.0 - t); }
            case 2u: { v = b + t; }
            default: { v = t; }
        }
        c[i] = recover(b + (v - b) * u.p1.y, u.p1.z);
    }
    return vec4<f32>(to_linear1(c.r) * o.a, to_linear1(c.g) * o.a, to_linear1(c.b) * o.a, o.a);
}

@compute @workgroup_size(16, 16)
fn ext_px(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= u.i0.y || id.y >= u.i0.z {
        return;
    }
    let p = vec2<i32>(id.xy);
    switch u.i1.x {
        case NODE_MID_DETAIL: {
            textureStore(dst, p, mid_detail(p));
            return;
        }
        case NODE_TO_SIGNAL: {
            let q = textureLoad(src, p, 0);
            if q.a <= 0.0 {
                textureStore(dst, p, vec4<f32>(0.0));
            } else {
                textureStore(dst, p, vec4<f32>(unpremul_signal(q) * q.a, q.a));
            }
            return;
        }
        case NODE_GAUSS: {
            textureStore(dst, p, gauss_pass(p));
            return;
        }
        case NODE_SHARPEN: {
            textureStore(dst, p, sharpen(p));
            return;
        }
        case NODE_GLOW_DOWN: {
            textureStore(dst, p, glow_down(p));
            return;
        }
        case NODE_GLOW_BLUR: {
            textureStore(dst, p, glow_blur(p));
            return;
        }
        case NODE_GLOW: {
            textureStore(dst, p, glow(p));
            return;
        }
        default: {}
    }
    let px = textureLoad(src, p, 0);
    let a = px.a;
    if a <= 0.0 {
        textureStore(dst, p, px);
        return;
    }
    let s = vec3<f32>(to_signal1(px.r / a), to_signal1(px.g / a), to_signal1(px.b / a));
    let g = node(s);
    textureStore(dst, p, vec4<f32>(to_linear1(g.r) * a, to_linear1(g.g) * a, to_linear1(g.b) * a, a));
}
