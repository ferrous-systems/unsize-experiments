use core::ptr::{DynMetadata, Pointee};

// use alloc::alloc::Global;
// use alloc::boxed::Box;

use crate::pointer::Pointer;
use crate::unsize::Unsize;

pub trait DispatchFromDyn<UnsizedSelf, SelfPointee, UnsizedSelfPointee>
where
    Self: Pointer<SelfPointee>,
    SelfPointee: Unsize<UnsizedSelfPointee> + ?Sized,
    UnsizedSelf: Pointer<UnsizedSelfPointee>,
    UnsizedSelfPointee: ?Sized + Pointee<Metadata = DynMetadata<UnsizedSelfPointee>>,
{
    fn wide_to_narrow(wide: UnsizedSelf) -> Self;
}

// impl<T: ?Sized + Unsize<U>, U: ?Sized> DispatchFromDyn<Box<U>, T, U> for Box<T, Global> {
//     fn wide_to_narrow(wide: Box<U>) -> Self {
//         todo!()
//     }
// }
