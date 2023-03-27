pub trait Pointer<Pointee: ?Sized>: Sized {}
impl<T: ?Sized> Pointer<T> for *const T {}
impl<T: ?Sized> Pointer<T> for *mut T {}
impl<'a, T: ?Sized> Pointer<T> for &'a T {}
impl<'a, T: ?Sized> Pointer<T> for &'a mut T {}
impl<T, U> Pointer<U> for core::pin::Pin<T>
where
    T: Pointer<U>,
    U: ?Sized,
{
}
impl<T: ?Sized> Pointer<T> for alloc::boxed::Box<T> {}
