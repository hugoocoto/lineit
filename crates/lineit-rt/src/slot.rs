//! Where a compiled program keeps state: the outputs of squares that run on
//! ticks, and variables.
//!
//! A program without fan-outs runs on one thread and keeps each as a
//! [`Local`], a plain field. One with fan-outs shares its state between
//! threads, so numbers and bools become atomics (relaxed: two branches that
//! change one at once can lose a change, as the language says, but nothing
//! tears) and everything else is behind a mutex. All of them have the same
//! methods, so the generated code is the same either way.
//!
//! `with` and `with_mut` take what they need and release the lock before
//! returning: the generated code computes the arguments first, so it never
//! holds two locks at once.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering::Relaxed};

#[derive(Default)]
pub struct Local<T>(pub T);

impl<T: Clone> Local<T> {
    #[inline(always)]
    pub fn new(v: T) -> Self {
        Local(v)
    }
    #[inline(always)]
    pub fn get(&self) -> T {
        self.0.clone()
    }
    #[inline(always)]
    pub fn set(&mut self, v: T) {
        self.0 = v;
    }
    #[inline(always)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.0)
    }
    #[inline(always)]
    pub fn with_mut<R>(&mut self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.0)
    }
}

pub struct Shared<T>(Mutex<T>);

impl<T: Clone> Shared<T> {
    pub fn new(v: T) -> Self {
        Shared(Mutex::new(v))
    }
    #[inline]
    pub fn get(&self) -> T {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    #[inline]
    pub fn set(&self, v: T) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = v;
    }
    #[inline]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
    #[inline]
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

pub struct SharedInt(AtomicI64);

impl SharedInt {
    pub fn new(v: i64) -> Self {
        SharedInt(AtomicI64::new(v))
    }
    #[inline(always)]
    pub fn get(&self) -> i64 {
        self.0.load(Relaxed)
    }
    #[inline(always)]
    pub fn set(&self, v: i64) {
        self.0.store(v, Relaxed)
    }
}

pub struct SharedFloat(AtomicU64);

impl SharedFloat {
    pub fn new(v: f64) -> Self {
        SharedFloat(AtomicU64::new(v.to_bits()))
    }
    #[inline(always)]
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Relaxed))
    }
    #[inline(always)]
    pub fn set(&self, v: f64) {
        self.0.store(v.to_bits(), Relaxed)
    }
}

pub struct SharedBool(AtomicBool);

impl SharedBool {
    pub fn new(v: bool) -> Self {
        SharedBool(AtomicBool::new(v))
    }
    #[inline(always)]
    pub fn get(&self) -> bool {
        self.0.load(Relaxed)
    }
    #[inline(always)]
    pub fn set(&self, v: bool) {
        self.0.store(v, Relaxed)
    }
}
