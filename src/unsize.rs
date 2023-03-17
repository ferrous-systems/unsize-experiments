use core::ptr::Pointee;

/// Types that can be "unsized" to a dynamically-sized type.
///
/// For example, the sized array type `[i8; 2]` implements `Unsize<[i8]>` and
/// `Unsize<dyn fmt::Debug>`.
///
/// The following implementations are provided by the compiler:
///
/// - Types implementing a trait `Trait` also implement `Unsize<dyn Trait>`.
/// - Structs `Foo<..., T, ...>` implement `Unsize<Foo<..., U, ...>>` if all of these conditions
///   are met:
///   - `T: Unsize<U>`.
///   - Only the last field of `Foo` has a type involving `T`.
///   - `Bar<T>: Unsize<Bar<U>>`, where `Bar<T>` stands for the actual type of that last field.
///
/// `Unsize` is used along with [`ops::CoerceUnsized`] to allow
/// "user-defined" containers such as [`Rc`] to contain dynamically-sized
/// types. See the [DST coercion RFC][RFC982] and [the nomicon entry on coercion][nomicon-coerce]
/// for more details.
///
/// [`ops::CoerceUnsized`]: crate::ops::CoerceUnsized
/// [`Rc`]: ../../std/rc/struct.Rc.html
/// [RFC982]: https://github.com/rust-lang/rfcs/blob/master/text/0982-dst-coercion.md
/// [nomicon-coerce]: ../../nomicon/coercions.html
/// SAFETY: Implementations of this trait require the output of [`Unsize::metadata`] and [`Unsize::data_address`] to belong
/// to the same object. In other words, creating a DST pointer out of these two outputs should result in a valid pointer.
// #[lang = "unsize"]
pub unsafe trait Unsize<Target>
where
    // ideally this would be !Sized
    Target: ?Sized,
{
    /// SAFETY: `self` must point to a valid instance of `Self`
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata;
    /// SAFETY: `self` must point to a valid instance of `Self` and the returned value must be valid (? there is more to this)
    unsafe fn target_data_address(self: *const Self) -> *const () {
        self.cast()
    }
}

unsafe impl<T, const N: usize> Unsize<[T]> for [T; N] {
    // Note: This could be a safe method with https://rust-lang.github.io/rfcs/3245-refined-impls.html
    // removing the restriction that the pointer has to be valid
    unsafe fn target_metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        N
    }
}

unsafe impl<T> Unsize<[T]> for alloc::vec::Vec<T> {
    unsafe fn target_metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        // Note, this would be `self.len`
        unsafe { (*self).len() }
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        // Note, this would be `self.buf.ptr.as_ptr().cast()`
        unsafe { (*self).as_ptr().cast() }
    }
}

/* the compiler will generate impls of the form:
unsafe impl<trait Trait, T: Trait> Unsize<dyn Trait> for T {
    // Note: This could be a safe method with https://rust-lang.github.io/rfcs/3245-refined-impls.html
    // removing the restriction that the pointer has to be valid
    unsafe fn metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        instrincis::vtable::<Trait>()
    }
}
*/

// Note that this impl is observable on stable, and we can get overlap with some explicit impls by users
/* the compiler will generate impls of the form:
unsafe impl<T, U> Unsize<Foo<..., U, ...>> for Foo<..., T, ...> where the rules apply that the docs currently state you know the drill ... {
    // Note: This could be a safe method with https://rust-lang.github.io/rfcs/3245-refined-impls.html
    // removing the restriction that the pointer has to be valid
    unsafe fn metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        unsafe { Unsize::metadata(&raw const (*self).<last_field>) }
    }
}
*/
