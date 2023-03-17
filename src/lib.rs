#![no_std]
#![allow(incomplete_features)]
#![forbid(unsafe_op_in_unsafe_fn)]
#![feature(
    ptr_metadata,
    arbitrary_self_types,
    allocator_api,
    unsafe_pin_internals
)]

extern crate alloc;

mod coerce_unsized;
mod dispatch_from_dyn;
mod pointer;
mod unsize;

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
        let slice: alloc::sync::Arc<[_]> = alloc::sync::Arc::new([10; 0]).coerce_unsized();
        assert_eq!(&*slice, &[10; 0][..]);
    }

    // This fails to compile as it should, Vec does not coerce with an unchanging address
    // #[test]
    // fn arc_is_unchanging() {
    //     let slice: alloc::sync::Arc<[_]> =
    //         alloc::sync::Arc::new(alloc::vec![[0; 10]; 10]).coerce_unsized();
    // }
}
