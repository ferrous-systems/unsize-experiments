use core::alloc::Allocator;
use core::pin::Pin;
use core::ptr;

use alloc::boxed::Box;

use crate::unsize::Unsize;

pub trait CoerceUnsized<Target>
// where // std has this bound for some reason?
//     Target: ?Sized,
{
    fn coerce_unsized(self) -> Target;
}

// &mut T -> &mut U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<&'a mut U> for &'a mut T {
    fn coerce_unsized(self) -> &'a mut U {
        unsafe {
            &mut *ptr::from_raw_parts_mut(
                Unsize::data_address(self).cast_mut(),
                Unsize::metadata(self),
            )
        }
    }
}

// &mut T -> &U
impl<'a, 'b: 'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<&'a U> for &'b mut T {
    fn coerce_unsized(self) -> &'a U {
        unsafe { &*ptr::from_raw_parts(Unsize::data_address(self), Unsize::metadata(self)) }
    }
}

// &mut T -> *mut U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*mut U> for &'a mut T {
    fn coerce_unsized(self) -> *mut U {
        ptr::from_raw_parts_mut(
            Unsize::data_address(self).cast_mut(),
            Unsize::metadata(self),
        )
    }
}

// &mut T -> *const U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*const U> for &'a mut T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(Unsize::data_address(self), Unsize::metadata(self))
    }
}

// &T -> &U
impl<'a, 'b: 'a, T: Unsize<U> + ?Sized, U: ?Sized> CoerceUnsized<&'a U> for &'b T {
    fn coerce_unsized(self) -> &'a U {
        unsafe { &*ptr::from_raw_parts(Unsize::data_address(self), Unsize::metadata(self)) }
    }
}

// &T -> *const U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*const U> for &'a T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(Unsize::data_address(self), Unsize::metadata(self))
    }
}

// *mut T -> *mut U
impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*mut U> for *mut T {
    fn coerce_unsized(self) -> *mut U {
        ptr::from_raw_parts_mut(
            Unsize::data_address(self).cast_mut(),
            Unsize::metadata(self),
        )
    }
}

// *mut T -> *const U
impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*const U> for *mut T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(Unsize::data_address(self), Unsize::metadata(self))
    }
}

// *const T -> *const U
impl<T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*const U> for *const T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(Unsize::data_address(self), Unsize::metadata(self))
    }
}

// Box<T> -> Box<U>
impl<T: ?Sized + Unsize<U>, U: ?Sized, A: Allocator> CoerceUnsized<Box<U, A>> for Box<T, A> {
    fn coerce_unsized(self) -> Box<U, A> {
        let (this, a) = Box::into_raw_with_allocator(self);
        unsafe {
            Box::from_raw_in(
                ptr::from_raw_parts_mut(
                    Unsize::data_address(this).cast_mut(),
                    Unsize::metadata(this),
                ),
                a,
            )
        }
    }
}

// Copied from core library docs:
// Note: this means that any impl of `CoerceUnsized` that allows coercing from
// a type that impls `Deref<Target=impl !Unpin>` to a type that impls
// `Deref<Target=Unpin>` is unsound. Any such impl would probably be unsound
// for other reasons, though, so we just need to take care not to allow such
// impls to land in std.
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P>
where
    P: CoerceUnsized<U>,
    // interesting one, we would need this for constructing the Pin via `new_unchecked`
    // U: core::ops::Deref,
{
    fn coerce_unsized(self) -> Pin<U> {
        Pin {
            pointer: self.pointer.coerce_unsized(),
        }
    }
}
