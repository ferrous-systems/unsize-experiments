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
    /// SAFETY: `self` must point to a valid instance of `Self` and the returned value must point to a valid object.
    unsafe fn target_data_address(self: *const Self) -> *const ();
}

/// A type that can be unsized solely through compile time information
/// SAFETY: The implementation of [`Unsize::target_data_address`] must return the input pointer.
pub unsafe trait StableUnsize<Target>: Unsize<Target>
where
    // ideally this would be !Sized
    Target: ?Sized,
{
}

/// A type that can be unsized solely through compile time information.
/// SAFETY: The implementation of [`Unsize::target_data_address`] must return the input pointer.
/// SAFETY: The implementation of [`Unsize::target_metadata`] must return the same as [`StaticUnsize::target_metadata`].
#[const_trait]
pub unsafe trait StaticUnsize<Target>: StableUnsize<Target>
where
    // ideally this would be !Sized
    Target: ?Sized,
{
    fn target_metadata() -> <Target as Pointee>::Metadata;
}

/// SAFETY: The metadata returned by `target_metadata` belongs to the object pointed to by the pointer returned by `target_address`
unsafe impl<T, const N: usize> Unsize<[T]> for [T; N] {
    unsafe fn target_metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        N
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        self.cast()
    }
}
/// SAFETY: `target_data_address` returns the input pointer
unsafe impl<T, const N: usize> StableUnsize<[T]> for [T; N] {}
/// SAFETY: `Unsize::target_metadata` returns the same value as `StaticUnsize::target_metadata`
unsafe impl<T, const N: usize> const StaticUnsize<[T]> for [T; N] {
    fn target_metadata() -> <[T] as Pointee>::Metadata {
        N
    }
}

/// SAFETY: The metadata returned by `target_metadata` belongs to the object pointed to by the pointer returned by `target_address`
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
    unsafe fn target_metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        intrinsics::vtable::<Trait>()
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        self.cast()
    }
}
unsafe impl<trait Trait, T: Trait> StableUnsize<dyn Trait> for T {}
unsafe impl<trait Trait, T: Trait> const StaticUnsize<dyn Trait> for T {
    fn target_metadata() -> <[T] as Pointee>::Metadata {
        intrinsics::vtable::<Trait>()
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
    unsafe fn target_data_address(self: *const Self) -> *const () {
        self.cast()
    }
}
unsafe impl<T, U> StableUnsize<Foo<..., U, ...>> for Foo<..., T, ...> where the rules apply that the docs currently state you know the drill ... {}
unsafe impl<T, U> const StaticUnsize<Foo<..., U, ...>> for Foo<..., T, ...> where the rules apply that the docs currently state you know the drill ... {
    fn target_metadata() -> <[T] as Pointee>::Metadata {
        intrinsics::vtable::<Trait>()
    }
}
*/
