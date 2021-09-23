//! This module provides unsafe ways to do some things
//!
#![allow(clippy::wildcard_imports)]

use std::alloc::*;
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::mem::*;
use std::ops::{Add, AddAssign, Mul, MulAssign, Sub};
/// An abstraction of an AVX ymm register that
///allows some things to not look ugly
#[derive(Clone, Copy)]
pub struct YmmRegister {
    /// An AVX register
    pub(crate) mm256: __m256i,
}

impl Add for YmmRegister {
    type Output = YmmRegister;
    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        unsafe {
            return YmmRegister {
                mm256: _mm256_add_epi32(self.mm256, rhs.mm256),
            };
        }
    }
}

impl Add<i32> for YmmRegister {
    type Output = YmmRegister;
    #[inline]
    fn add(self, rhs: i32) -> Self::Output {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);
            return YmmRegister {
                mm256: _mm256_add_epi32(self.mm256, tmp),
            };
        }
    }
}

impl Sub for YmmRegister {
    type Output = YmmRegister;
    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        unsafe {
            return YmmRegister {
                mm256: _mm256_sub_epi32(self.mm256, rhs.mm256),
            };
        }
    }
}

impl AddAssign for YmmRegister {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        unsafe {
            self.mm256 = _mm256_add_epi32(self.mm256, rhs.mm256);
        }
    }
}

impl AddAssign<i32> for YmmRegister {
    #[inline]
    fn add_assign(&mut self, rhs: i32) {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);
            self.mm256 = _mm256_add_epi32(self.mm256, tmp);
        }
    }
}

impl Mul for YmmRegister {
    type Output = YmmRegister;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        unsafe {
            YmmRegister {
                mm256: _mm256_mullo_epi32(self.mm256, rhs.mm256),
            }
        }
    }
}

impl Mul<i32> for YmmRegister {
    type Output = YmmRegister;
    #[inline]
    fn mul(self, rhs: i32) -> Self::Output {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);
            YmmRegister {
                mm256: _mm256_mullo_epi32(self.mm256, tmp),
            }
        }
    }
}

impl MulAssign for YmmRegister {
    #[inline]
    fn mul_assign(&mut self, rhs: Self) {
        unsafe {
            self.mm256 = _mm256_mullo_epi32(self.mm256, rhs.mm256);
        }
    }
}

impl MulAssign<i32> for YmmRegister {
    #[inline]
    fn mul_assign(&mut self, rhs: i32) {
        unsafe {
            let tmp = _mm256_set1_epi32(rhs);
            self.mm256 = _mm256_mullo_epi32(self.mm256, tmp);
        }
    }
}

impl MulAssign<__m256i> for YmmRegister {
    #[inline]
    fn mul_assign(&mut self, rhs: __m256i) {
        unsafe {
            self.mm256 = _mm256_mullo_epi32(self.mm256, rhs);
        }
    }
}

/// Allocate a Vec<T> with a ALIGN byte boundary and zero its content
///
/// # Invariants the caller must uphold
///  - Do not reallocate a vector created from here
///     reallocation causes it to change its alignment to `mem::align_of::<T>()`
///     which might not be the same as `ALIGN`
///  - Do not use a smaller alignment than a type, please consult `mem::size_of<T>()` for sizes
///  - Do not use this function when drunk, doing so may cause the universe to implode
///
/// # Why?
/// - Most AVX and SSE instructions require memory allocated to a certain byte boundary
/// this allows us to use streaming stores and streaming loads where it makes sense
///
/// - Also aligned loads are faster than unaligned loads
///
///  # Returns
///  A `Vec<T>` which should contain space for at least `capacity` items
///
/// The size allocated is actually `mem::size_of::<T>()*capacity`
///
///# Examples
///```
/// fn check(){
///    unsafe{
///        // align a vec of i32 to a 32 byte boundary with all elements initialized to 0
///        let vector = align_zero_alloc::<i32,32>(200);
///        assert!(vector,vec![0;200]);
///    }
/// }
///```
pub(crate) unsafe fn align_zero_alloc<T, const ALIGNMENT: usize>(capacity: usize) -> Vec<T>
where
    T: Default + Copy,
{
    // Create a new layout, with alignment Align
    let layout = Layout::from_size_align(capacity * size_of::<T>(), ALIGNMENT)
        .expect("Error creating memory alignment.");
    // Call alloc_zeroed, this returns zeroed memory
    let ptr = alloc_zeroed(layout);
    // Call Vec to handle pointer stuff
    // Safety variants checked..
    //  1:ptr is not allocated via string/ Vec<T> but via alloc which both
    // structs use internally
    // 2: Length and capacity are the same
    Vec::<T>::from_raw_parts(ptr.cast(), capacity, capacity)
}
