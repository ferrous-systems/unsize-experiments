//! This module experiments with a new Unsize definition, splitting it into four [`Unsize`],
//! [`StableUnsize`], [`FromMetadataUnsize`] and [`ConstUnsize`]. Where [`ConstUnsize`] is effectively today's `Unsize` trait.
use core::ptr::Pointee;

// Note there was `ConstUnsize` trait before that had an associated constant for the metadata instead
// but that trait is technically unnecessary, it is effectively `FromMetadataUnsize<Target>` where
// the target_metadata function is const and `Self::Metadata = ()`. Once const traits land this therefor
// serves no usecase.

/// Types that can be "unsized" to a dynamically-sized type.
///
/// For example, the sized array type `[i8; 2]` implements `Unsize<[i8]>` and
/// `Unsize<dyn fmt::Debug>`.
///
/// The following implementations are provided by the compiler:
///
/// - Types implementing a trait `Trait` also implement `Unsize<dyn Trait>`.
/// - Trait objects `dyn Trait` with supertrait `Super` implement `Unsize<dyn Super>`.
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
/// - The implementation of [`Unsize::target_metadata`] must return metadata that is valid for
/// the object pointed to by the output of [`Unsize::target_data_address`].
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
    /// `self` must point to a valid instance of `Self`.
    // Note: This should effectively allow to field project (or return self) only?
    unsafe fn target_data_address(self: *const Self) -> *const ();
}

/// Same as [`Unsize`] but the target data address may not change.
///
/// # Safety
///
/// - The implementation of [`StableUnsize::target_metadata`] must return metadata that is valid for
/// the object pointed to by the `self` parameter
/// - The implementing type and [`Target`] must be layout compatible.
pub unsafe trait StableUnsize<Target>: Unsize<Target>
where
    Target: ?Sized,
{
    /// # Safety
    ///
    /// `self` must point to a valid instance of `Self`
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata;
}

/// # Safety
///
/// - The implementation of [`StableUnsize::target_metadata`] must return metadata that is valid for
/// the object pointed to by the `self` parameter
/// - The implementing type and [`Target`] must be layout compatible.
pub unsafe trait FromMetadataUnsize<Target>: StableUnsize<Target>
where
    Target: ?Sized,
{
    fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <Target as Pointee>::Metadata;
}

// StableUnsize implies Unsize!
// SAFETY: `target_metadata` returns valid metadata for the `target_data_address` result, as per
// `StableUnsize::target_metadata` implementation
unsafe impl<T, Target> Unsize<Target> for T
where
    Target: ?Sized,
    T: StableUnsize<Target> + ?Sized,
{
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata {
        // SAFETY: `self` points to a valid object of Self as per the calling contract of `Unsize::target_metadata`
        unsafe { <Self as StableUnsize<Target>>::target_metadata(self) }
    }

    unsafe fn target_data_address(self: *const Self) -> *const () {
        self.cast()
    }
}

// FromMetadataUnsize implies StableUnsize!
// SAFETY:
// - The implementation of [`StableUnsize::target_metadata`] returns metadata that is valid for
// all objects of type `Target` as per `FromMetadataUnsize`
// - The implementing type and [`Target`] are layout compatible as per `FromMetadataUnsize`.
unsafe impl<T, Target> StableUnsize<Target> for T
where
    Target: ?Sized,
    T: FromMetadataUnsize<Target> + ?Sized,
{
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata {
        <Self as FromMetadataUnsize<Target>>::target_metadata(core::ptr::metadata(self))
    }
}

// SAFETY: `Unsize::target_metadata` returns the same value as `ConstUnsize::TARGET_METADATA`
unsafe impl<T, const N: usize> FromMetadataUnsize<[T]> for [T; N] {
    fn target_metadata((): <Self as Pointee>::Metadata) -> <[T] as Pointee>::Metadata {
        N
    }
}

// SAFETY: The metadata returned by `target_metadata` belongs to the object pointed to by the pointer returned by `target_address`
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

// SAFETY: The metadata returned by `target_metadata` belongs to the object pointed to by the pointer returned by `target_address`
unsafe impl Unsize<str> for alloc::string::String {
    unsafe fn target_metadata(self: *const Self) -> <str as Pointee>::Metadata {
        // SAFETY: self is a valid pointer
        unsafe { (*self).len() }
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        // SAFETY: self is a valid pointer
        unsafe { (*self).as_ptr().cast() }
    }
}

/* the compiler will generate impls of the form:
unsafe impl<trait Trait, T: Trait> ConstUnsize<dyn Trait> for T {
    const TARGET_METADATA: <dyn Trait as core::ptr::Pointee>::Metadata = intrinsics::vtable::<dyn Trait, T>();
}
Note that we require T to be sized here! otherwise we would lose the metadata of the source and more importantly,
`str` could be coerced into trait objects which is not a thing today
*/

/* trait upcasting: the compiler will generate impls of the form:
unsafe impl<trait Trait, trait Super> FromMetadataUnsize<dyn Super> for dyn Trait where dyn Trait: Super {
    unsafe fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <dyn super as Pointee>::Metadata {
        // some compiler magic
    }
}
*/

// Note that this impl is observable on stable rust already
/* the compiler will generate impls of the form:
unsafe impl<T, U> ConstUnsize<Foo<U>> for Foo<T>
where
    U: ?Sized,
    // bound for the generic param that is being coerced
    T: ConstUnsize<U>,
    // bound for the type of the last field that is being coerced
    Bar<T>: ConstUnsize<Bar<U>>,
    // demand that the metadata for Foo is the same as its last field
    Foo<U>: core::ptr::Pointee<Metadata = <Bar<U> as core::ptr::Pointee>::Metadata>,
{
    const TARGET_METADATA: <Foo<U> as core::ptr::Pointee>::Metadata = <Bar<T> as ConstUnsize<Bar<U>>>::TARGET_METADATA;
}
*/
