pub trait Pointer<Pointee: ?Sized> {}
impl<T: ?Sized> Pointer<T> for *const T {}
