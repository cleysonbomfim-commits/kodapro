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
        default: {
            return c;
        }
    }
}

@compute @workgroup_size(16, 16)
fn ext_px(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= u.i0.y || id.y >= u.i0.z {
        return;
    }
    let p = vec2<i32>(id.xy);
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
