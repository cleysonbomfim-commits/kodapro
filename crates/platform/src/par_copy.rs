//! Row-parallel copies out of a mapped biplanar picture (a luma plane, then interleaved Cb/Cr):
//! the luma rows as they are, the chroma rows split into two planes, 16-bit samples shifted down
//! from the high bits. Safe code, tested on every target.
//!
//! A 4K 10-bit 4:2:2 picture is ~33 MB of samples; copying it on one thread took most of the
//! ~86 ms of CPU per frame VideoToolbox pictures cost (Sony FX30 XAVC S, `docs/koda/DIARIO.md`).
//! The rows are independent, so they are copied on the rayon pool in bands of rows.

use rayon::prelude::*;

/// Rows per rayon task: enough work per task that scheduling stays negligible.
const BAND: usize = 16;

/// A mapped plane: `stride` bytes per row.
#[derive(Clone, Copy)]
pub struct Plane<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

impl<'a> Plane<'a> {
    /// `len` bytes of row `y` from byte `x`.
    fn row(&self, y: usize, x: usize, len: usize) -> Result<&'a [u8], String> {
        let start = y.checked_mul(self.stride).and_then(|s| s.checked_add(x)).ok_or("plane offset overflows")?;
        let end = start.checked_add(len).ok_or("plane offset overflows")?;
        self.data.get(start..end).ok_or_else(|| "plane row out of range".to_string())
    }
}

/// Rows `y0..y0 + rows`, `w` samples each from sample `x0`, of an 8-bit plane.
pub fn copy_u8(src: Plane, x0: usize, y0: usize, w: usize, rows: usize) -> Result<Vec<u8>, String> {
    let mut out = filmcraft_frame::pool::take_u8(w.checked_mul(rows).ok_or("picture size overflows")?);
    out.resize(w * rows, 0);
    if w == 0 {
        return Ok(out);
    }
    out.par_chunks_mut(w).with_min_len(BAND).enumerate().try_for_each(|(y, dst)| {
        dst.copy_from_slice(src.row(y0 + y, x0, w)?);
        Ok::<(), String>(())
    })?;
    Ok(out)
}

/// [`copy_u8`] for 16-bit samples (native byte order), each shifted right by `shift`.
pub fn copy_u16(src: Plane, x0: usize, y0: usize, w: usize, rows: usize, shift: u32) -> Result<Vec<u16>, String> {
    let mut out = filmcraft_frame::pool::take_u16(w.checked_mul(rows).ok_or("picture size overflows")?);
    out.resize(w * rows, 0);
    if w == 0 {
        return Ok(out);
    }
    out.par_chunks_mut(w).with_min_len(BAND).enumerate().try_for_each(|(y, dst)| {
        let s = src.row(y0 + y, x0 * 2, w * 2)?;
        for (d, b) in dst.iter_mut().zip(s.as_chunks::<2>().0) {
            *d = u16::from_ne_bytes(*b) >> shift;
        }
        Ok::<(), String>(())
    })?;
    Ok(out)
}

/// Rows of interleaved 8-bit Cb/Cr pairs, `w` pairs each from pair `x0`, split into two planes.
pub fn split_u8(src: Plane, x0: usize, y0: usize, w: usize, rows: usize) -> Result<(Vec<u8>, Vec<u8>), String> {
    let n = w.checked_mul(rows).ok_or("picture size overflows")?;
    let (mut u, mut v) = (filmcraft_frame::pool::take_u8(n), filmcraft_frame::pool::take_u8(n));
    u.resize(n, 0);
    v.resize(n, 0);
    if w == 0 {
        return Ok((u, v));
    }
    u.par_chunks_mut(w).zip(v.par_chunks_mut(w)).with_min_len(BAND).enumerate().try_for_each(|(y, (du, dv))| {
        let pairs = src.row(y0 + y, x0 * 2, w * 2)?.as_chunks::<2>().0;
        for ((a, b), p) in du.iter_mut().zip(dv.iter_mut()).zip(pairs) {
            *a = p[0];
            *b = p[1];
        }
        Ok::<(), String>(())
    })?;
    Ok((u, v))
}

/// [`split_u8`] for 16-bit samples (native byte order), each shifted right by `shift`.
pub fn split_u16(src: Plane, x0: usize, y0: usize, w: usize, rows: usize, shift: u32) -> Result<(Vec<u16>, Vec<u16>), String> {
    let n = w.checked_mul(rows).ok_or("picture size overflows")?;
    let (mut u, mut v) = (filmcraft_frame::pool::take_u16(n), filmcraft_frame::pool::take_u16(n));
    u.resize(n, 0);
    v.resize(n, 0);
    if w == 0 {
        return Ok((u, v));
    }
    u.par_chunks_mut(w).zip(v.par_chunks_mut(w)).with_min_len(BAND).enumerate().try_for_each(|(y, (du, dv))| {
        let quads = src.row(y0 + y, x0 * 4, w * 4)?.as_chunks::<4>().0;
        for ((a, b), q) in du.iter_mut().zip(dv.iter_mut()).zip(quads) {
            *a = u16::from_ne_bytes([q[0], q[1]]) >> shift;
            *b = u16::from_ne_bytes([q[2], q[3]]) >> shift;
        }
        Ok::<(), String>(())
    })?;
    Ok((u, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane of `rows` rows, `stride` bytes each, filled with a pattern unique per byte.
    fn pattern(stride: usize, rows: usize) -> Vec<u8> {
        (0..stride * rows).map(|i| (i.wrapping_mul(31) ^ (i >> 7)) as u8).collect()
    }

    /// The single-threaded copy `videotoolbox::copy_out` used before, as the reference.
    fn serial_u16(p: Plane, x0: usize, y0: usize, w: usize, rows: usize, shift: u32) -> Vec<u16> {
        let mut out = Vec::new();
        for y in 0..rows {
            let r = &p.data[(y0 + y) * p.stride + x0 * 2..][..w * 2];
            out.extend(r.as_chunks::<2>().0.iter().map(|b| u16::from_ne_bytes(*b) >> shift));
        }
        out
    }

    #[test]
    fn parallel_copies_match_the_serial_copy() {
        // 4:2:2-like: 300 luma rows (many bands), a crop offset, padded stride
        let (stride, rows) = (1000, 300);
        let data = pattern(stride, rows);
        let p = Plane { data: &data, stride };
        let (x0, y0, w, h) = (3, 5, 480, 290);
        assert_eq!(copy_u16(p, x0, y0, w, h, 6).unwrap(), serial_u16(p, x0, y0, w, h, 6));
        let y8 = copy_u8(p, x0, y0, w, h).unwrap();
        assert_eq!(y8.len(), w * h);
        assert_eq!(&y8[w * 17..w * 18], &data[(y0 + 17) * stride + x0..][..w]);
        // chroma: w / 2 pairs per row
        let (u, v) = split_u16(p, 1, y0, w / 2, h, 6).unwrap();
        let (mut su, mut sv) = (Vec::new(), Vec::new());
        for y in 0..h {
            for q in data[(y0 + y) * stride + 4..][..w * 2].as_chunks::<4>().0 {
                su.push(u16::from_ne_bytes([q[0], q[1]]) >> 6);
                sv.push(u16::from_ne_bytes([q[2], q[3]]) >> 6);
            }
        }
        assert_eq!((u, v), (su, sv));
        let (u8s, v8s) = split_u8(p, 2, 0, 10, 40).unwrap();
        assert_eq!(u8s[10 * 39 + 3], data[39 * stride + (2 + 3) * 2]);
        assert_eq!(v8s[10 * 39 + 3], data[39 * stride + (2 + 3) * 2 + 1]);
    }

    #[test]
    fn short_planes_are_errors_not_panics() {
        let data = pattern(64, 10);
        let p = Plane { data: &data, stride: 64 };
        assert!(copy_u16(p, 0, 0, 40, 10, 6).is_err(), "rows wider than the stride");
        assert!(copy_u8(p, 0, 5, 64, 10).is_err(), "more rows than the plane has");
        assert!(split_u16(p, 0, 0, 20, usize::MAX / 8, 6).is_err(), "size overflow");
        assert!(copy_u8(p, 0, 0, 0, 10).unwrap().is_empty());
    }
}
