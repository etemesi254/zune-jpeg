#![cfg(feature = "x86")]
//! This module provides unsafe ways to do some things
#![allow(clippy::wildcard_imports)]

use std::alloc::{alloc, Layout};
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::mem::size_of;
use std::ops::{Add, AddAssign, Mul, MulAssign, Sub};

/// An abstraction of an AVX ymm register that
///allows some things to not look ugly
#[derive(Clone, Copy)]

pub struct YmmRegister
{
    /// An AVX register
    pub(crate) mm256: __m256i,
}

impl Add for YmmRegister
{
    type Output = YmmRegister;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output
    {
        unsafe {
            return YmmRegister {
                mm256: _mm256_add_epi32(self.mm256, rhs.mm256),
            };
        }
    }
}

impl Add<i32> for YmmRegister
{
    type Output = YmmRegister;

    #[inline]
    fn add(self, rhs: i32) -> Self::Output
    {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);

            return YmmRegister {
                mm256: _mm256_add_epi32(self.mm256, tmp),
            };
        }
    }
}

impl Sub for YmmRegister
{
    type Output = YmmRegister;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output
    {
        unsafe {
            return YmmRegister {
                mm256: _mm256_sub_epi32(self.mm256, rhs.mm256),
            };
        }
    }
}

impl AddAssign for YmmRegister
{
    #[inline]
    fn add_assign(&mut self, rhs: Self)
    {
        unsafe {
            self.mm256 = _mm256_add_epi32(self.mm256, rhs.mm256);
        }
    }
}

impl AddAssign<i32> for YmmRegister
{
    #[inline]
    fn add_assign(&mut self, rhs: i32)
    {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);

            self.mm256 = _mm256_add_epi32(self.mm256, tmp);
        }
    }
}

impl Mul for YmmRegister
{
    type Output = YmmRegister;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output
    {
        unsafe {
            YmmRegister {
                mm256: _mm256_mullo_epi32(self.mm256, rhs.mm256),
            }
        }
    }
}

impl Mul<i32> for YmmRegister
{
    type Output = YmmRegister;

    #[inline]
    fn mul(self, rhs: i32) -> Self::Output
    {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);

            YmmRegister {
                mm256: _mm256_mullo_epi32(self.mm256, tmp),
            }
        }
    }
}

impl MulAssign for YmmRegister
{
    #[inline]
    fn mul_assign(&mut self, rhs: Self)
    {
        unsafe {
            self.mm256 = _mm256_mullo_epi32(self.mm256, rhs.mm256);
        }
    }
}

impl MulAssign<i32> for YmmRegister
{
    #[inline]
    fn mul_assign(&mut self, rhs: i32)
    {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);

            self.mm256 = _mm256_mullo_epi32(self.mm256, tmp);
        }
    }
}

impl MulAssign<__m256i> for YmmRegister
{
    #[inline]
    fn mul_assign(&mut self, rhs: __m256i)
    {
        unsafe {
            self.mm256 = _mm256_mullo_epi32(self.mm256, rhs);
        }
    }
}
/// THIS CREATES A NEW VECTOR WITH INITIALIZED MEMORY AND SETS
/// THE LENGTH TO BE EQUAL TO THE CAPACITY...
///
/// DO NOT READ THE VALUES BEFORE A PRIOR WRITE.
///
/// **I'M SERIOUS ON THIS ONE.**
#[allow(clippy::expect_used)]
#[inline]
pub(crate) unsafe fn align_alloc<T, const ALIGNMENT: usize>(capacity: usize) -> Vec<T>
where
    T: Default + Copy,
{
    // Create a new layout
    let layout = Layout::from_size_align(capacity * size_of::<T>(), ALIGNMENT)
        .expect("Error creating memory alignment.");

    // Call alloc this returns uninit memory
    let ptr = alloc(layout);

    // This is cheating, IT will allocate uninit memory
    // But it's important because  we can do some cool optimizations with this.
    // if it were to change some things will panic.
    Vec::<T>::from_raw_parts(ptr.cast(), capacity, capacity)
}
