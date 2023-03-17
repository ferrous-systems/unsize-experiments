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
    use crate::coerce_unsized::CoerceUnsized;

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
    fn ui() {
        let t = trybuild::TestCases::new();
        t.compile_fail("tests/ui/*.rs");
    }
}
