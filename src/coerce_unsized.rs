use core::alloc::Allocator;
use core::cell::Cell;
use core::pin::Pin;
use core::ptr;

use alloc::boxed::Box;
use alloc::sync::Arc;

use crate::unsize::{ConstUnsize, StableUnsize, Unsize};
use crate::TypedMetadata;
/// Trait that indicates that this is a pointer or a wrapper for one,
/// where unsizing can be performed on the pointee.
///
/// See the [DST coercion RFC][dst-coerce] and [the nomicon entry on coercion][nomicon-coerce]
/// for more details.
///
/// For builtin pointer types, pointers to `T` will coerce to pointers to `U` if `T: Unsize<U>`
/// by converting from a thin pointer to a fat pointer.
///
/// For custom types, the coercion here works by coercing `Foo<T>` to `Foo<U>`
/// provided an impl of `CoerceUnsized<Foo<U>> for Foo<T>` exists.
/// Such an impl can only be written if `Foo<T>` has only a single non-phantomdata
/// field involving `T`. If the type of that field is `Bar<T>`, an implementation
/// of `CoerceUnsized<Bar<U>> for Bar<T>` must exist. The coercion will work by
/// coercing the `Bar<T>` field into `Bar<U>` and filling in the rest of the fields
/// from `Foo<T>` to create a `Foo<U>`. This will effectively drill down to a pointer
/// field and coerce that.
///
/// Generally, for smart pointers you will implement
/// `CoerceUnsized<Ptr<U>> for Ptr<T> where T: Unsize<U>, U: ?Sized`, with an
/// optional `?Sized` bound on `T` itself. For wrapper types that directly embed `T`
/// like `Cell<T>` and `RefCell<T>`, you
/// can directly implement `CoerceUnsized<Wrap<U>> for Wrap<T> where T: CoerceUnsized<U>`.
/// This will let coercions of types like `Cell<Box<T>>` work.
///
/// [`Unsize`][unsize] is used to mark types which can be coerced to DSTs if behind
/// pointers. It is implemented automatically by the compiler.
///
/// [dst-coerce]: https://github.com/rust-lang/rfcs/blob/master/text/0982-dst-coercion.md
/// [unsize]: crate::marker::Unsize
/// [nomicon-coerce]: ../../nomicon/coercions.html
// std has a `Target:?Sized` bound for some reason, but it's technically not required for any coercions,
// since Target is supposed to be a pointer or wrapper to a pointer
// TODO: This definition currently permits users to do some funky stuff that isn't using unsizing at all! Can we somehow restrict that?
// There are basically two kinds of implementations we want to allow
// - Delegation a la Cell or Pin (wrapper types), where we do the coercion for the inner type and then rewrap it
// - The actual coercion, which happens on pointer types
//
// assuming std had a Pointer trait, we could restrict Self and Target to this trait, and in case for Cell and Pin (and similar),
// have conditional implementations for this trait on them if their inner type also implements the trait, as effectively they still act like pointers
// We can't make Deref work for this, as raw pointers don't implement it
pub trait CoerceUnsized<Target> {
    fn coerce_unsized(self) -> Target;
}

/*
 * Here are the primitive pointer impls from core
 */

// &mut T -> &mut U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<&'a mut U> for &'a mut T {
    fn coerce_unsized(self) -> &'a mut U {
        // SAFETY: the returned fat pointer must be valid according to [`Unsize`]
        unsafe {
            &mut *ptr::from_raw_parts_mut(
                // SAFETY: self is a reference
                Unsize::target_data_address(self).cast_mut(),
                // SAFETY: self is a reference
                Unsize::target_metadata(self),
            )
        }
    }
}

// &mut T -> &U
impl<'a, 'b: 'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<&'a U> for &'b mut T {
    fn coerce_unsized(self) -> &'a U {
        // SAFETY: the returned fat pointer must be valid according to [`Unsize`]
        unsafe {
            &*ptr::from_raw_parts(
                // SAFETY: self is a reference
                Unsize::target_data_address(self),
                // SAFETY: self is a reference
                Unsize::target_metadata(self),
            )
        }
    }
}

// &mut T -> *mut U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*mut U> for &'a mut T {
    fn coerce_unsized(self) -> *mut U {
        ptr::from_raw_parts_mut(
            // SAFETY: self is a reference
            unsafe { Unsize::target_data_address(self) }.cast_mut(),
            // SAFETY: self is a reference
            unsafe { Unsize::target_metadata(self) },
        )
    }
}

// &mut T -> *const U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*const U> for &'a mut T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(
            // SAFETY: self is a reference
            unsafe { Unsize::target_data_address(self) },
            // SAFETY: self is a reference
            unsafe { Unsize::target_metadata(self) },
        )
    }
}

// &T -> &U
impl<'a, 'b: 'a, T: Unsize<U> + ?Sized, U: ?Sized> CoerceUnsized<&'a U> for &'b T {
    fn coerce_unsized(self) -> &'a U {
        // SAFETY: the returned fat pointer must be valid according to [`Unsize`]
        unsafe {
            &*ptr::from_raw_parts(
                // SAFETY: self is a reference
                Unsize::target_data_address(self),
                // SAFETY: self is a reference
                Unsize::target_metadata(self),
            )
        }
    }
}

// &T -> *const U
impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<*const U> for &'a T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(
            // SAFETY: self is a reference
            unsafe { Unsize::target_data_address(self) },
            // SAFETY: self is a reference
            unsafe { Unsize::target_metadata(self) },
        )
    }
}

// *mut T -> *mut U
// Note the use of ConstUnsize! We can't deref the pointer as we do not know whether it is live
impl<T: ?Sized + ConstUnsize<U>, U: ?Sized> CoerceUnsized<*mut U> for *mut T {
    fn coerce_unsized(self) -> *mut U {
        ptr::from_raw_parts_mut(self.cast(), <T as ConstUnsize<U>>::TARGET_METADATA)
    }
}

// *mut T -> *const U
// Note the use of ConstUnsize! We can't deref the pointer as we do not know whether it is live
impl<T: ?Sized + ConstUnsize<U>, U: ?Sized> CoerceUnsized<*const U> for *mut T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(self.cast(), <T as ConstUnsize<U>>::TARGET_METADATA)
    }
}

// *const T -> *const U
// Note the use of ConstUnsize! We can't deref the pointer as we do not know whether it is live
impl<T: ?Sized + ConstUnsize<U>, U: ?Sized> CoerceUnsized<*const U> for *const T {
    fn coerce_unsized(self) -> *const U {
        ptr::from_raw_parts(self.cast(), <T as ConstUnsize<U>>::TARGET_METADATA)
    }
}

/*
 * Some more interesting implementations
 */

// Box<T> -> Box<U>
// Note the use of StableUnsize! unstable unsize would be unsound as arc relies on the data pointer pointing inside of the ArcInner.
impl<T: ?Sized + StableUnsize<U>, U: ?Sized, A: Allocator> CoerceUnsized<Box<U, A>> for Box<T, A> {
    fn coerce_unsized(self) -> Box<U, A> {
        let (this, a) = Box::into_raw_with_allocator(self);
        // SAFETY: According to [`StableUnsize`] the metadata is associated with our pointer
        unsafe {
            Box::from_raw_in(
                ptr::from_raw_parts_mut(
                    this.cast(),
                    // SAFETY: this is derived from the box which is currently live
                    Unsize::target_metadata(this),
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

impl<T: CoerceUnsized<U>, U> CoerceUnsized<Cell<U>> for Cell<T> {
    fn coerce_unsized(self) -> Cell<U> {
        Cell::new(self.into_inner().coerce_unsized())
    }
}

// Note the use of StableUnsize! unstable unsize would be unsound as arc relies on the data pointer pointing inside of the ArcInner.
impl<T: ?Sized + StableUnsize<U>, U: ?Sized> CoerceUnsized<Arc<U>> for Arc<T> {
    fn coerce_unsized(self) -> Arc<U> {
        let ptr = Arc::into_raw(self);
        // SAFETY: The arc is safe to be constructed as the pointer is unchanged
        unsafe {
            Arc::from_raw(ptr::from_raw_parts(
                ptr.cast(),
                // SAFETY: ptr is derived from a live Arc and is therefor valid
                Unsize::target_metadata(ptr),
            ))
        }
    }
}

impl<T, U> CoerceUnsized<TypedMetadata<U>> for TypedMetadata<T>
where
    T: ?Sized + ConstUnsize<U>,
    U: ?Sized,
{
    fn coerce_unsized(self) -> TypedMetadata<U> {
        TypedMetadata(T::TARGET_METADATA)
    }
}
