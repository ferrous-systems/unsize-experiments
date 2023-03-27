#![no_std]
#![allow(incomplete_features)]
#![forbid(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]
#![feature(
    ptr_metadata,
    arbitrary_self_types,
    allocator_api,
    unsafe_pin_internals
)]

extern crate alloc;

pub mod coerce_unsized;
pub mod dispatch_from_dyn;
pub mod pointer;
pub mod unsize;

#[cfg(test)]
mod tests {
    use thin_vec::ThinVec;

    use crate::coerce_unsized::CoerceUnsized;
    use crate::unsize::{StaticUnsize, Unsize};

    use super::*;

    #[test]
    fn smoke_it() {
        let concrete = &alloc::vec![0; 10];
        let slice: &[_] = concrete.coerce_unsized();
        assert_eq!(slice, &[0; 10]);
    }

    #[test]
    fn smoke_it2() {
        let concrete = &alloc::vec![alloc::vec![0; 10]; 10];
        let slice: &[_] = concrete.coerce_unsized();
        assert_eq!(
            slice,
            &core::array::from_fn::<_, 10, _>(|_| alloc::vec![0; 10])
        );
    }

    #[test]
    fn arc_it() {
        let slice: alloc::sync::Arc<[_]> = alloc::sync::Arc::new([0; 10]).coerce_unsized();
        assert_eq!(&*slice, &[0; 10][..]);
    }

    #[test]
    fn static_unsize() {
        let _: *const [_] = (&[0; 10] as *const [i32; 10]).coerce_unsized();
    }

    #[test]
    fn fixed_str() {
        #[repr(transparent)]
        struct FixedString<const N: usize>([u8; N]);

        unsafe impl<const N: usize> StaticUnsize<str> for FixedString<N> {
            fn target_metadata() -> <str as core::ptr::Pointee>::Metadata {
                N
            }
        }
        let concrete = FixedString(*b"foo");
        let coerced: &str = (&concrete).coerce_unsized();
        assert_eq!(coerced, "foo");
    }

    #[test]
    fn thin_vec() {
        unsafe impl<T> Unsize<[T]> for ThinVec<T> {
            unsafe fn target_metadata(self: *const Self) -> <[T] as core::ptr::Pointee>::Metadata {
                unsafe { (*self).len() }
            }

            unsafe fn target_data_address(self: *const Self) -> *const () {
                unsafe { (*self).as_ptr().cast() }
            }
        }
        let concrete = thin_vec::thin_vec![0; 10];
        let coerced: &[_] = (&concrete).coerce_unsized();
        assert_eq!(coerced, &[0; 10][..]);
    }

    #[test]
    fn to_dyn_trait_coerce() {
        trait Trait {
            fn as_string(&self) -> alloc::string::String;
        }
        impl Trait for i32 {
            fn as_string(&self) -> alloc::string::String {
                alloc::string::ToString::to_string(self)
            }
        }
        // emulate the compiler impl
        unsafe impl StaticUnsize<dyn Trait> for i32 {
            fn target_metadata() -> <dyn Trait as core::ptr::Pointee>::Metadata {
                core::ptr::metadata::<dyn Trait>(&0 as *const _ as *const _)
            }
        }
        let concrete = 0;
        let coerced: &dyn Trait = (&concrete).coerce_unsized();
        assert_eq!(
            coerced.as_string(),
            alloc::string::ToString::to_string(&concrete)
        );
    }

    #[test]
    #[cfg(not(miri))]
    fn ui() {
        let t = trybuild::TestCases::new();
        t.compile_fail("tests/ui/*.rs");
    }
}
