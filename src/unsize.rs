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
/// # Safety
///
/// Implementations of this trait require the output of [`Unsize::target_metadata`] and [`Unsize::target_data_address`] to belong
/// to the same object. In other words, creating a DST pointer out of these two outputs should result in a valid pointer.
///
/// [`ops::CoerceUnsized`]: crate::ops::CoerceUnsized
/// [`Rc`]: ../../std/rc/struct.Rc.html
/// [RFC982]: https://github.com/rust-lang/rfcs/blob/master/text/0982-dst-coercion.md
/// [nomicon-coerce]: ../../nomicon/coercions.html
// #[lang = "unsize"]
pub unsafe trait Unsize<Target>
where
    // ideally this would be !Sized
    Target: ?Sized,
{
    /// # Safety
    ///
    /// `self` must point to a valid instance of `Self`
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata;
    /// # Safety
    ///
    /// `self` must point to a valid instance of `Self` and the returned value must point to a valid object.
    // Note: This should effectively allow to field project (or return self) only.
    unsafe fn target_data_address(self: *const Self) -> *const ();
}

/// A type that can be unsized solely through compile time information
///
/// # Safety
///
/// The implementation of [`Unsize::target_data_address`] must return the input pointer.
pub unsafe trait StableUnsize<Target>: Unsize<Target>
where
    // ideally this would be !Sized
    Target: ?Sized,
{
}

/// A type that can be unsized solely through compile time information.
///
/// # Safety
///
/// The implementation of [`StaticUnsize::target_metadata`] must return metadata that is valid for
/// any object that represents the [`Target`] type.
pub unsafe trait StaticUnsize<Target>: StableUnsize<Target>
where
    // ideally this would be !Sized
    Target: ?Sized,
{
    // This should probably be a `const`?
    fn target_metadata() -> <Target as Pointee>::Metadata;
}

// StaticUnsize implies Unsize!
unsafe impl<T, Target> Unsize<Target> for T
where
    Target: ?Sized,
    T: StaticUnsize<Target>,
{
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata {
        <Self as StaticUnsize<Target>>::target_metadata()
    }

    unsafe fn target_data_address(self: *const Self) -> *const () {
        self.cast()
    }
}
// StaticUnsize implies StableUnsize!
unsafe impl<T, Target> StableUnsize<Target> for T
where
    Target: ?Sized,
    T: StaticUnsize<Target>,
{
}

/// SAFETY: `Unsize::target_metadata` returns the same value as `StaticUnsize::target_metadata`
unsafe impl<T, const N: usize> StaticUnsize<[T]> for [T; N] {
    fn target_metadata() -> <[T] as Pointee>::Metadata {
        N
    }
}

/// SAFETY: The metadata returned by `target_metadata` belongs to the object pointed to by the pointer returned by `target_address`
unsafe impl<T> Unsize<[T]> for alloc::vec::Vec<T> {
    unsafe fn target_metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        // SAFETY: self is a valid pointer
        unsafe { (*self).len() }
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        // SAFETY: self is a valid pointer
        unsafe { (*self).as_ptr().cast() }
    }
}

/* the compiler will generate impls of the form:
unsafe impl<trait Trait, T: Trait> StaticUnsize<dyn Trait> for T {
    fn target_metadata() -> <[T] as Pointee>::Metadata {
        intrinsics::vtable::<Trait>()
    }
}
*/

// Note that this impl is observable on stable, and we can get overlap with some explicit impls by users
/* the compiler will generate impls of the form:
unsafe impl<T, U> StaticUnsize<Foo<..., U, ...>> for Foo<..., T, ...> where the rules apply that the docs currently state you know the drill ... {
    fn target_metadata() -> <[T] as Pointee>::Metadata {
        intrinsics::vtable::<Trait>()
    }
}
*/
