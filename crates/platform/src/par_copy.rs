//! Row-parallel copies out of a mapped biplanar picture (a luma plane, then interleaved Cb/Cr):
//! the luma rows as they are, the chroma rows split into two planes, 16-bit samples shifted down
//! from the high bits. Safe code, tested on every target.
//!
//! A 4K 10-bit 4:2:2 picture is ~33 MB of samples; copying it on one thread took most of the
//! ~86 ms of CPU per frame VideoToolbox pictures cost (Sony FX30 XAVC S, `docs/koda/DIARIO.md`).
//! The rows are independent, so they are copied in bands on a few **scoped threads of their own**,
//! never on the rayon pool: these copies run inside VideoToolbox's output callback, and during an
//! export every rayon worker can be waiting for that very decoder (render batch → GOP cache →
//! decode → callback). Handing the copy to the pool then deadlocked the export (two workflow tests
//! hung for 12 hours, 2026-10-16). Scoped threads wait on nothing but their own rows.

/// Rows a thread copies at least.
const BAND: usize = 16;
/// Most threads one copy uses.
const MAX_THREADS: usize = 8;

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

/// Threads for `rows` rows: one per [`BAND`] rows, at most the cores and [`MAX_THREADS`].
fn threads_for(rows: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_THREADS);
    (rows / BAND).clamp(1, cores)
}

/// Collect scoped threads' results: the first error, or a panic reported as an error.
fn joined<'s>(handles: Vec<std::thread::ScopedJoinHandle<'s, Result<(), String>>>) -> Result<(), String> {
    let mut out = Ok(());
    for h in handles {
        let r = h.join().unwrap_or_else(|_| Err("a picture copy thread panicked".to_string()));
        if out.is_ok() {
            out = r;
        }
    }
    out
}

/// Run `f(row, out_row)` for every `w`-wide row of `out`, in bands on scoped threads (on this
/// thread when a thread can't be started).
fn bands<T: Send>(out: &mut [T], w: usize, f: &(dyn Fn(usize, &mut [T]) -> Result<(), String> + Sync)) -> Result<(), String> {
    if w == 0 {
        return Ok(());
    }
    let n = threads_for(out.len() / w);
    let per = (out.len() / w).div_ceil(n).max(1);
    let run = |first: usize, part: &mut [T]| -> Result<(), String> {
        for (r, row) in part.chunks_mut(w).enumerate() {
            f(first + r, row)?;
        }
        Ok(())
    };
    if n == 1 {
        return run(0, out);
    }
    let run = &run;
    let mut spawned = true;
    let res = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for (i, part) in out.chunks_mut(per * w).enumerate() {
            match std::thread::Builder::new().name("frame-copy".into()).spawn_scoped(s, move || run(i * per, part)) {
                Ok(h) => handles.push(h),
                Err(_) => spawned = false,
            }
        }
        joined(handles)
    });
    // a band whose thread could not start is copied again with everything, here
    if spawned { res } else { run(0, out) }
}

/// [`bands`] over two planes of the same shape (chroma Cb and Cr).
fn bands2<T: Send>(a: &mut [T], b: &mut [T], w: usize, f: &(dyn Fn(usize, &mut [T], &mut [T]) -> Result<(), String> + Sync)) -> Result<(), String> {
    if w == 0 || a.len() != b.len() {
        return if w == 0 { Ok(()) } else { Err("chroma planes differ in size".into()) };
    }
    let n = threads_for(a.len() / w);
    let per = (a.len() / w).div_ceil(n).max(1);
    let run = |first: usize, pa: &mut [T], pb: &mut [T]| -> Result<(), String> {
        for (r, (ra, rb)) in pa.chunks_mut(w).zip(pb.chunks_mut(w)).enumerate() {
            f(first + r, ra, rb)?;
        }
        Ok(())
    };
    if n == 1 {
        return run(0, a, b);
    }
    let run = &run;
    let mut spawned = true;
    let res = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for (i, (pa, pb)) in a.chunks_mut(per * w).zip(b.chunks_mut(per * w)).enumerate() {
            match std::thread::Builder::new().name("frame-copy".into()).spawn_scoped(s, move || run(i * per, pa, pb)) {
                Ok(h) => handles.push(h),
                Err(_) => spawned = false,
            }
        }
        joined(handles)
    });
    if spawned { res } else { run(0, a, b) }
}

/// Rows `y0..y0 + rows`, `w` samples each from sample `x0`, of an 8-bit plane.
pub fn copy_u8(src: Plane, x0: usize, y0: usize, w: usize, rows: usize) -> Result<Vec<u8>, String> {
    let mut out = filmcraft_frame::pool::take_u8(w.checked_mul(rows).ok_or("picture size overflows")?);
    out.resize(w * rows, 0);
    bands(&mut out, w, &|y, dst| {
        dst.copy_from_slice(src.row(y0 + y, x0, w)?);
        Ok(())
    })?;
    Ok(out)
}

/// [`copy_u8`] for 16-bit samples (native byte order), each shifted right by `shift`.
pub fn copy_u16(src: Plane, x0: usize, y0: usize, w: usize, rows: usize, shift: u32) -> Result<Vec<u16>, String> {
    let mut out = filmcraft_frame::pool::take_u16(w.checked_mul(rows).ok_or("picture size overflows")?);
    out.resize(w * rows, 0);
    bands(&mut out, w, &|y, dst| {
        let s = src.row(y0 + y, x0 * 2, w * 2)?;
        for (d, b) in dst.iter_mut().zip(s.as_chunks::<2>().0) {
            *d = u16::from_ne_bytes(*b) >> shift;
        }
        Ok(())
    })?;
    Ok(out)
}

/// Rows of interleaved 8-bit Cb/Cr pairs, `w` pairs each from pair `x0`, split into two planes.
pub fn split_u8(src: Plane, x0: usize, y0: usize, w: usize, rows: usize) -> Result<(Vec<u8>, Vec<u8>), String> {
    let n = w.checked_mul(rows).ok_or("picture size overflows")?;
    let (mut u, mut v) = (filmcraft_frame::pool::take_u8(n), filmcraft_frame::pool::take_u8(n));
    u.resize(n, 0);
    v.resize(n, 0);
    bands2(&mut u, &mut v, w, &|y, du, dv| {
        let pairs = src.row(y0 + y, x0 * 2, w * 2)?.as_chunks::<2>().0;
        for ((a, b), p) in du.iter_mut().zip(dv.iter_mut()).zip(pairs) {
            *a = p[0];
            *b = p[1];
        }
        Ok(())
    })?;
    Ok((u, v))
}

/// [`split_u8`] for 16-bit samples (native byte order), each shifted right by `shift`.
pub fn split_u16(src: Plane, x0: usize, y0: usize, w: usize, rows: usize, shift: u32) -> Result<(Vec<u16>, Vec<u16>), String> {
    let n = w.checked_mul(rows).ok_or("picture size overflows")?;
    let (mut u, mut v) = (filmcraft_frame::pool::take_u16(n), filmcraft_frame::pool::take_u16(n));
    u.resize(n, 0);
    v.resize(n, 0);
    bands2(&mut u, &mut v, w, &|y, du, dv| {
        let quads = src.row(y0 + y, x0 * 4, w * 4)?.as_chunks::<4>().0;
        for ((a, b), q) in du.iter_mut().zip(dv.iter_mut()).zip(quads) {
            *a = u16::from_ne_bytes([q[0], q[1]]) >> shift;
            *b = u16::from_ne_bytes([q[2], q[3]]) >> shift;
        }
        Ok(())
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

    /// Regression (2026-10-16): a copy made from outside the rayon pool (VideoToolbox's output
    /// callback) while every rayon worker waits for that copy's decoder must still finish. On the
    /// rayon pool it waited for the busy workers forever: an export deadlocked.
    #[test]
    fn copies_never_wait_for_a_busy_rayon_pool() {
        use std::sync::{Arc, Condvar, Mutex};
        use std::time::Duration;
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        // occupy every rayon worker until the copy is done (as export workers wait for a decode)
        for _ in 0..rayon::current_num_threads() {
            let g = gate.clone();
            rayon::spawn(move || {
                let (m, c) = &*g;
                let mut open = m.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                while !*open {
                    open = c.wait(open).unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            });
        }
        // the decoder's own thread copies a 4K-sized picture
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let data = pattern(7680, 400);
            let p = Plane { data: &data, stride: 7680 };
            let _ = tx.send(copy_u16(p, 0, 0, 3840, 400, 6).map(|v| v.len()).and_then(|n| split_u16(p, 0, 0, 1920, 400, 6).map(|(u, _)| n + u.len())));
        });
        let got = rx.recv_timeout(Duration::from_secs(30));
        // release the workers whatever happened
        let (m, c) = &*gate;
        *m.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        c.notify_all();
        assert_eq!(got.expect("the copy waited for busy rayon workers (deadlock)"), Ok(3840 * 400 + 1920 * 400));
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
