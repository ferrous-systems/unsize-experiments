use core::ptr::Pointee;

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
    // ideally this would be !Sized
    Target: ?Sized,
{
    /// # Safety
    ///
    /// `self` must point to a valid instance of `Self`
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata;
}

/// A type that can be unsized solely through compile time information.
///
/// # Safety
///
/// - The implementation of [`ConstUnsize::TARGET_METADATA`] must return metadata that is valid for
/// any object that represents the [`Target`] type.
/// - The implementing type and [`Target`] must be layout compatible.
pub unsafe trait ConstUnsize<Target>: StableUnsize<Target>
where
    // ideally this would be !Sized
    Target: ?Sized,
{
    const TARGET_METADATA: <Target as Pointee>::Metadata;
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
// ConstUnsize implies StableUnsize!
// SAFETY:
// - The implementation of [`StableUnsize::target_metadata`] returns metadata that is valid for
// all objects of type `Target` as per `ConstUnsize`
// - The implementing type and [`Target`] are layout compatible as per `ConstUnsize`.
unsafe impl<T, Target> StableUnsize<Target> for T
where
    Target: ?Sized,
    T: ConstUnsize<Target> + ?Sized,
{
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata {
        <Self as ConstUnsize<Target>>::TARGET_METADATA
    }
}

// SAFETY: `Unsize::target_metadata` returns the same value as `ConstUnsize::TARGET_METADATA`
unsafe impl<T, const N: usize> ConstUnsize<[T]> for [T; N] {
    const TARGET_METADATA: <[T] as Pointee>::Metadata = N;
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
*/

/* trait upcasting: the compiler will generate impls of the form:
unsafe impl<trait Trait, trait Super> StableUnsize<dyn Super> for dyn Trait where dyn Trait: Super {
    fn target_metadata() -> <dyn super as Pointee>::Metadata {
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
