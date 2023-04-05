// use alloc::alloc::Global;
// use alloc::boxed::Box;

use core::ptr::{DynMetadata, Pointee};

use alloc::boxed::Box;

use crate::unsize::Unsize;

pub trait DispatchFromDyn<UnsizedSelf>
// where
// UnsizedSelf: crate::pointer::Pointer<impl Pointee<Metadata = DynMetadata>>,
{
    fn wide_to_narrow(wide: UnsizedSelf) -> Self;
    // fn vtable(&self) -> DynMetadata;
}

impl<P, U> DispatchFromDyn<core::pin::Pin<U>> for core::pin::Pin<P>
where
    P: DispatchFromDyn<U>,
{
    fn wide_to_narrow(wide: core::pin::Pin<U>) -> Self {
        core::pin::Pin {
            pointer: DispatchFromDyn::wide_to_narrow(wide.pointer),
        }
    }
}

impl<'a, T, U> DispatchFromDyn<&'a U> for &'a T
where
    T: Unsize<U> + Sized,
    U: Pointee<Metadata = DynMetadata<U>>,
    // U: ?Sized, std does this, but this is technically wrong? You cannot dispatch from wide pointer to wide pointer
    // as you will lose the initial metadata!
{
    fn wide_to_narrow(wide: &'a U) -> Self {
        let address = (wide as *const U).to_raw_parts().0;
        // SAFETY: ??
        unsafe { &*address.cast() }
    }
}

impl<T, U> DispatchFromDyn<Box<U>> for Box<T>
where
    T: Unsize<U> + Sized,
    U: Pointee<Metadata = DynMetadata<U>>,
    // U: ?Sized,
{
    fn wide_to_narrow(wide: Box<U>) -> Self {
        let address = Box::into_raw(wide).to_raw_parts().0;
        // SAFETY: ??
        unsafe { Box::from_raw(address.cast()) }
    }
}

// https://internals.rust-lang.org/t/rc-arc-borrowed-an-object-safe-version-of-rc-t-arc-t/8896/4
// such an impl unfortunately conflicts
// impl<T, U> DispatchFromDyn<&Box<U>> for &Box<T>
// where
//     T: ?Sized + Unsize<U>,
//     U: Pointee<Metadata = DynMetadata<U>>,
//     // U: ?Sized,
// {
//     fn wide_to_narrow(wide: &Box<U>) -> Self {
//         // assume layout of wide to be `&(Box<T>, &VTable)
//         unsafe { &*(wide as *const Box<U> as *const Self) }
//     }
// }
