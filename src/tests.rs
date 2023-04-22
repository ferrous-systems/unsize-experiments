use core::ptr::addr_of;

use thin_vec::ThinVec;

use crate::coerce_unsized::CoerceUnsized;
use crate::unsize::{ConstUnsize, FromMetadataUnsize, Unsize};

use super::*;

#[test]
fn smoke_it() {
    let concrete = &alloc::vec![0; 10];
    let slice: &[_] = concrete.coerce_unsized();
    assert_eq!(slice, &[0; 10]);
}

#[test]
fn smoke_it2() {
    let concrete = &alloc::vec![alloc::vec![0; 10]; 10];
    let slice: &[_] = concrete.coerce_unsized();
    assert_eq!(
        slice,
        &core::array::from_fn::<_, 10, _>(|_| alloc::vec![0; 10])
    );
}

#[test]
fn arc_it() {
    let slice: alloc::sync::Arc<[_]> = alloc::sync::Arc::new([0; 10]).coerce_unsized();
    assert_eq!(&*slice, &[0; 10][..]);
}

#[test]
fn const_unsize_ptr() {
    let coerced: *const [_] = (&[0; 10] as *const [i32; 10]).coerce_unsized();
    // SAFETY: coerced points to a live slice
    assert_eq!(unsafe { &*coerced }, &[0; 10][..]);
}

#[test]
fn fixed_str() {
    #[repr(transparent)]
    struct FixedString<const N: usize>([u8; N]);

    // SAFETY: The metadata returned by `target_metadata` is valid for a `str` object representing the `Self` object
    unsafe impl<const N: usize> ConstUnsize<str> for FixedString<N> {
        const TARGET_METADATA: <str as core::ptr::Pointee>::Metadata = N;
    }
    let concrete = FixedString(*b"foo");
    let coerced: &str = (&concrete).coerce_unsized();
    assert_eq!(coerced, "foo");
}

#[test]
fn fixed_str_dyn_len() {
    struct FixedStringWithLen<const N: usize>(usize, [u8; N]);

    // SAFETY: The metadata returned by `target_metadata` is valid for a `str` object representing the `Self` object
    unsafe impl<const N: usize> Unsize<str> for FixedStringWithLen<N> {
        unsafe fn target_metadata(self: *const Self) -> <str as core::ptr::Pointee>::Metadata {
            // SAFETY: self points to a live Self
            let len = unsafe { (*self).0 };
            assert!(len <= N);
            len
        }

        unsafe fn target_data_address(self: *const Self) -> *const () {
            // SAFETY: self points to a live Self
            unsafe { addr_of!((*self).1).cast() }
        }
    }
    let concrete = FixedStringWithLen(3, *b"foo\0\0\0\0\0");
    let coerced: &str = (&concrete).coerce_unsized();
    assert_eq!(coerced, "foo");
}

#[test]
fn thin_vec() {
    // SAFETY: The metadata returned is valid for the data pointer returned by target_data_address
    unsafe impl<T> Unsize<[T]> for ThinVec<T> {
        unsafe fn target_metadata(self: *const Self) -> <[T] as core::ptr::Pointee>::Metadata {
            // SAFETY: self points a live Self as per calling contract
            unsafe { (*self).len() }
        }

        unsafe fn target_data_address(self: *const Self) -> *const () {
            // SAFETY: self points a live Self as per calling contract
            unsafe { (*self).as_ptr().cast() }
        }
    }
    let concrete = thin_vec::thin_vec![0; 10];
    let coerced: &[_] = (&concrete).coerce_unsized();
    assert_eq!(coerced, &[0; 10][..]);
}

#[test]
fn to_dyn_trait_coerce() {
    trait Trait {
        fn as_string(&self) -> alloc::string::String;
    }
    impl Trait for i32 {
        fn as_string(&self) -> alloc::string::String {
            alloc::string::ToString::to_string(self)
        }
    }
    // emulate the compiler impl
    // SAFETY: i32 and dyn Trait are layout compatible as i32 implements Trait and the metadata produced is a valid vtable for dyn Trait
    unsafe impl ConstUnsize<dyn Trait> for i32 {
        const TARGET_METADATA: <dyn Trait as core::ptr::Pointee>::Metadata =
            core::ptr::metadata::<dyn Trait>(&0 as *const _ as *const _);
    }
    let concrete = 0;
    let coerced: &dyn Trait = (&concrete).coerce_unsized();
    assert_eq!(
        coerced.as_string(),
        alloc::string::ToString::to_string(&concrete)
    );
}
#[test]
fn to_dyn_trait_coerce_upcast() {
    trait Super {
        fn as_super_string(&self) -> alloc::string::String;
    }
    trait Trait: Super {}
    impl Super for i32 {
        fn as_super_string(&self) -> alloc::string::String {
            alloc::string::ToString::to_string(self)
        }
    }
    impl Trait for i32 {}
    // emulate the compiler impl
    // SAFETY: i32 and dyn Trait are layout compatible as i32 implements Trait and the metadata produced is a valid vtable for dyn Trait
    unsafe impl ConstUnsize<dyn Trait> for i32 {
        const TARGET_METADATA: <dyn Trait as core::ptr::Pointee>::Metadata =
            core::ptr::metadata::<dyn Trait>(&0 as *const _ as *const _);
    }
    // emulate the compiler impl
    // SAFETY:
    unsafe impl FromMetadataUnsize<dyn Super> for dyn Trait {
        fn target_metadata(
            _: <Self as core::ptr::Pointee>::Metadata,
        ) -> <dyn Super as core::ptr::Pointee>::Metadata {
            // This isn't really correct obviously, but there is no proper to emulate what the compiler does here
            core::ptr::metadata(&0i32 as &dyn Super)
        }
    }
    let concrete = 0;
    // ref
    let coerced: &dyn Trait = (&concrete).coerce_unsized();
    let coerced: &dyn Super = coerced.coerce_unsized();
    assert_eq!(
        coerced.as_super_string(),
        alloc::string::ToString::to_string(&concrete)
    );
    // raw ptr
    let coerced: *const dyn Trait = (&concrete as *const i32).coerce_unsized();
    let coerced: *const dyn Super = coerced.coerce_unsized();
    assert_eq!(
        // SAFETY: The pointer is still valid
        unsafe { (*coerced).as_super_string() },
        alloc::string::ToString::to_string(&concrete)
    );
}

#[test]
fn compiler_adt_builtin_coerce() {
    struct Foo<T: ?Sized> {
        field: Bar<T>,
    }
    struct Bar<T: ?Sized> {
        field: T,
    }
    // emulate the compiler impls
    // SAFETY: field is the last field of Foo, so the layout is stable
    unsafe impl<T, U> ConstUnsize<Foo<U>> for Foo<T>
    where
        U: ?Sized,                   // bound for the generic param that is being coerced
        T: ConstUnsize<U>,           // bound for the generic param that is being coerced
        Bar<T>: ConstUnsize<Bar<U>>, // bound derived from Foo's last field
        Foo<U>: core::ptr::Pointee<Metadata = <Bar<U> as core::ptr::Pointee>::Metadata>, // bound requiring the metadata of the struct and its field to be the same
    {
        const TARGET_METADATA: <Foo<U> as core::ptr::Pointee>::Metadata =
            <Bar<T> as ConstUnsize<Bar<U>>>::TARGET_METADATA;
    }
    // SAFETY: field is the last field of Bar, so the layout is stable
    unsafe impl<T, U> ConstUnsize<Bar<U>> for Bar<T>
    where
        U: ?Sized,         // bound for the generic param that is being coerced
        T: ConstUnsize<U>, // bound for the generic param that is being coerced
        T: ConstUnsize<U>, // bound derived from Bar's last field
        Bar<U>: core::ptr::Pointee<Metadata = <U as core::ptr::Pointee>::Metadata>, // bound requiring the metadata of the struct and its field to be the same
    {
        const TARGET_METADATA: <Bar<U> as core::ptr::Pointee>::Metadata =
            <T as ConstUnsize<U>>::TARGET_METADATA;
    }
    let concrete = Foo {
        field: Bar { field: [0; 10] },
    };
    let coerced: &Foo<[i32]> = (&concrete).coerce_unsized();
    assert_eq!(&coerced.field.field, &[0; 10][..]);
}

#[test]
fn coerce_type_metadata() {
    struct Struct;
    trait Trait {}

    impl Trait for Struct {}
    // SAFETY: This would be a compiler provided impl
    unsafe impl ConstUnsize<dyn Trait> for Struct {
        const TARGET_METADATA: <dyn Trait as core::ptr::Pointee>::Metadata =
            core::ptr::metadata::<dyn Trait>(&Struct as *const _ as *const _);
    }

    // array -> slice
    let sized: TypedMetadata<[u8; 5]> = TypedMetadata(());
    let _: TypedMetadata<[u8]> = sized.coerce_unsized();

    // sized -> dyn
    let sized: TypedMetadata<Struct> = TypedMetadata(());
    let _: TypedMetadata<dyn Trait> = sized.coerce_unsized();
}

#[test]
fn option_coerce() {
    #[derive(PartialEq, Debug)]
    enum Option<T> {
        Some(T),
        None,
    }

    impl<T, U> CoerceUnsized<Option<U>> for Option<T>
    where
        T: CoerceUnsized<U>,
    {
        fn coerce_unsized(self) -> Option<U> {
            match self {
                Option::Some(t) => Option::Some(t.coerce_unsized()),
                Option::None => Option::None,
            }
        }
    }

    let concrete = Option::Some(&[0u8; 1]);
    let coerced: Option<&[u8]> = concrete.coerce_unsized();
    assert_eq!(coerced, Option::Some(&[0][..]));
    let concrete = Option::None::<&[u8; 1]>;
    let coerced: Option<&[u8]> = concrete.coerce_unsized();
    assert_eq!(coerced, Option::None);
}

// https://github.com/rust-lang/rfcs/pull/1792
#[test]
fn coerce_ref_option_inner() {
    #[derive(PartialEq, Debug)]
    enum Option<T> {
        Some(T),
        None,
    }

    impl<T, U> CoerceUnsized<Option<U>> for Option<T>
    where
        T: CoerceUnsized<U>,
    {
        fn coerce_unsized(self) -> Option<U> {
            match self {
                Option::Some(t) => Option::Some(t.coerce_unsized()),
                Option::None => Option::None,
            }
        }
    }

    // T: Vec<T>
    // U: [U]
    impl<'a, 'b, T, U: ?Sized> CoerceUnsized<Option<&'b U>> for &'a Option<T>
    where
        &'a T: CoerceUnsized<&'b U>,
    {
        fn coerce_unsized(self) -> Option<&'b U> {
            match self {
                Option::Some(t) => Option::Some(t.coerce_unsized()),
                Option::None => Option::None,
            }
        }
    }

    let concrete = Option::Some(alloc::vec![0, 1, 2, 3, 4]);
    let coerced: Option<&[i32]> = (&concrete).coerce_unsized();
    assert_eq!(coerced, Option::Some(&[0, 1, 2, 3, 4][..]));
    let concrete = Option::None::<alloc::vec::Vec<i32>>;
    let coerced: Option<&[i32]> = (&concrete).coerce_unsized();
    assert_eq!(coerced, Option::None);
}

#[test]
#[cfg(not(miri))]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
