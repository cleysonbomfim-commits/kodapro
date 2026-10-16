// KODA Pro grade nodes (koda-color), run by filmcraft-gpu's effect stage as external passes.
// Bindings and uniform layout are fx.wgsl's: the working image arrives in `src` (linear
// premultiplied RGBA f32) and the result goes to `dst`. One pass = one node: u.i1.x is the node
// type, u.p0..p5 its parameters. The math mirrors koda_color::eval::apply_cpu.

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

const NODE_GAIN: u32 = 1u;

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

fn node(c: vec3<f32>) -> vec3<f32> {
    switch u.i1.x {
        case NODE_GAIN: {
            return c * u.p0.xyz;
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
