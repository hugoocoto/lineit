//! Random numbers: xoshiro256** per thread, seeded from the clock and the
//! thread, or with [`seed`] for runs that repeat.

use std::cell::Cell;

thread_local! {
    static STATE: Cell<[u64; 4]> = Cell::new(initial());
}

fn splitmix(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn from_seed(mut s: u64) -> [u64; 4] {
    [splitmix(&mut s), splitmix(&mut s), splitmix(&mut s), splitmix(&mut s)]
}

fn initial() -> [u64; 4] {
    let t =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let local = 0u8;
    from_seed(t ^ (&local as *const u8 as u64).rotate_left(32))
}

/// Seeds this thread's generator.
pub fn seed(s: u64) {
    STATE.with(|c| c.set(from_seed(s)));
}

pub fn next() -> u64 {
    STATE.with(|c| {
        let mut s = c.get();
        let r = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        c.set(s);
        r
    })
}

/// An int from LO to HI, both included.
pub fn int(lo: i64, hi: i64) -> i64 {
    if hi <= lo {
        return lo;
    }
    let span = (hi as i128 - lo as i128 + 1) as u128;
    (lo as i128 + (next() as u128 % span) as i128) as i64
}

/// A float from LO to HI (HI left out).
pub fn float(lo: f64, hi: f64) -> f64 {
    let u = (next() >> 11) as f64 / (1u64 << 53) as f64;
    lo + (hi - lo) * u
}
