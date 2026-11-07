//! Lists and maps. An index out of range reads the type's default and
//! writes nothing; a negative index counts from the end.

use std::collections::BTreeMap;

#[inline]
fn index(len: usize, i: i64) -> Option<usize> {
    let i = if i < 0 { len as i64 + i } else { i };
    if i >= 0 && (i as usize) < len { Some(i as usize) } else { None }
}

#[inline]
pub fn get<T: Clone + Default>(l: &[T], i: i64) -> T {
    index(l.len(), i).map(|i| l[i].clone()).unwrap_or_default()
}

#[inline]
pub fn set<T>(l: &mut [T], i: i64, v: T) {
    if let Some(i) = index(l.len(), i) {
        l[i] = v;
    }
}

#[inline]
pub fn remove<T>(l: &mut Vec<T>, i: i64) {
    if let Some(i) = index(l.len(), i) {
        l.remove(i);
    }
}

/// Inserts V before index I (at the end if I is the length or more).
#[inline]
pub fn insert<T>(l: &mut Vec<T>, i: i64, v: T) {
    let i = if i < 0 { (l.len() as i64 + i + 1).max(0) as usize } else { (i as usize).min(l.len()) };
    l.insert(i, v);
}

#[inline]
pub fn pop<T: Default>(l: &mut Vec<T>) -> T {
    l.pop().unwrap_or_default()
}

#[inline]
pub fn map_get<T: Clone + Default>(m: &BTreeMap<String, T>, k: &str) -> T {
    m.get(k).cloned().unwrap_or_default()
}

pub fn map_keys<T>(m: &BTreeMap<String, T>) -> Vec<String> {
    m.keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes() {
        let mut l = vec![1i64, 2, 3];
        assert_eq!(get(&l, -1), 3);
        assert_eq!(get(&l, 3), 0);
        set(&mut l, 5, 9);
        assert_eq!(l, [1, 2, 3]);
        insert(&mut l, -1, 4);
        assert_eq!(l, [1, 2, 3, 4]);
        remove(&mut l, 0);
        assert_eq!(pop(&mut l), 4);
        assert_eq!(l, [2, 3]);
    }
}
