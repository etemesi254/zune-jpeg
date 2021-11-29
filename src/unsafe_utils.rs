#![cfg(feature = "x86")]
#![cfg(any(target_arch = "x86",target_arch = "x86_64"))]
//! This module provides unsafe ways to do some things
#![allow(clippy::wildcard_imports)]

use std::alloc::{alloc_zeroed, Layout};
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

/// Create an aligned vector whose start byte is aligned to an
/// ALIGNMENT boundary.
///
/// # Panics
/// In  in case alignment is not a power of 2 or zero.
///
#[allow(clippy::expect_used)]
#[inline]
pub(crate) unsafe fn align_alloc<T, const ALIGNMENT: usize>(capacity: usize) -> Vec<T>
where
    T: Default + Copy,
{
    // check alignment, since we are passing it as a const parameter, the compiler either
    // generates panicking code or  goes and allocates so this is effectively a no-op.
    assert!(
        ALIGNMENT.is_power_of_two() && ALIGNMENT != 0,
        "Alignment constrains for memory not met. Alignment is {} not a power of two",
        ALIGNMENT
    );

    // Create a new layout
    let layout = Layout::from_size_align_unchecked(capacity * size_of::<T>(), ALIGNMENT);

    // Use alloc zeroed to prevent page faults .So let me talk about this

    // The kernel mmap modifies page tables when calling  malloc, but doesn't allocate actual memory
    // This is an optimization technique because allocating memory that won't  be used is wasteful.
    // During writing to a new page, a page fault is triggered and the code moves to kernel space.
    // The kernel maps the page to memory and returns to user space.

    // This is ideally wasteful, if the memory will be written after being allocated, so what alloc_zeroed
    // does is trigger all page faults before we actually write to ensure we have mapped memory.
    //
    // Although it appears in flame-graphs as taking a lot of time to call, trust me it's better than
    // page faults every 4 KB's
    let ptr = alloc_zeroed(layout);

    Vec::<T>::from_raw_parts(ptr.cast(), capacity, capacity)
}

//-------------------------
// TEST CODE
//-------------------------
#[test]
fn test_aligned_32()
{
    unsafe {
        // allocate a vector of length 100
        let v = align_alloc::<i16, 64>(100);
        let ptr_start = v.as_ptr() as usize;
        println!("{}", ptr_start);
        assert_eq!(ptr_start % 64, 0);
    }
}
#[test]
#[should_panic]
fn test_aligned_alloc_panic()
{
    unsafe {
        // allocate a vector of length 100
        // fails because of alignment should be a power of two.
        align_alloc::<i16, 45>(100);
    }
}
