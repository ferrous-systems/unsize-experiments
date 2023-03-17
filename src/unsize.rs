use core::ptr::Pointee;

/// SAFETY: Implementations of this trait require the output of [`Unsize::metadata`] and [`Unsize::data_address`] to belong
/// to the same object.
pub unsafe trait Unsize<Target>
where
    Target: ?Sized,
{
    fn metadata(self: *const Self) -> <Target as Pointee>::Metadata;
    fn data_address(self: *const Self) -> *const () {
        self.cast()
    }
}

unsafe impl<T, const N: usize> Unsize<[T]> for [T; N] {
    fn metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        N
    }
}

unsafe impl<T> Unsize<[T]> for alloc::vec::Vec<T> {
    fn metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        unsafe { (*self).len() }
    }
    fn data_address(self: *const Self) -> *const () {
        unsafe { (*self).as_ptr().cast() }
    }
}

/* the compiler will generate impls of the form:
unsafe impl<trait Trait, T: Trait> Unsize<dyn Trait> for T {
    fn metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        vtable::<Trait>
    }
}
*/

// What about nested cases like Wrapper<Vec<T>> -> Wrapper<[T]>! what about Vec<Vec<T>>? That should stop at [Vec<T>], which means we have overlapping impls?

/* the compiler will generate impls of the form:
unsafe impl<T, U> Unsize<Foo<..., U, ...>> for Foo<..., T, ...> where the rules apply that the docs currently state you know the drill ... {
    fn metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        // uuh how do we fetch the metadata here, we need to pointer chase ...?
        unsafe { Unsize::metadata(&raw const  (*self).<last_field>) }
    }
}
*/
