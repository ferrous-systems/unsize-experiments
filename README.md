# Unsize and CoerceUnsize v2

This repo experiments with a new design for the `Unsize` and `CoerceUnsized` traits, making them more flexible and applicable to more types.
The motivation is to see how flexible the traits could be made and whether this flexibility pays off or not.
An idea that kicked this off was to have `Vec<T>: Unsize<[T]>`, which requires user code to run to be able to adjust the pointer for the data.
Today `Vec<T>` implements `Deref<Target = [T]>` for convenience as this allows dispatching slice methods on objects of `Vec<T>`, ye `Deref` was meant for smart pointers instead. What this impl really feels like is more akin to unsizing. Obviously the `Deref` impl cannot be revoked from `Vec<T>` anymore, but assuming an unsizing impl would've been the proper call this would then also imply that unsizing would have to be introduced into autoderef (instead of just special casing array unsizing).

What follows here is the accompanied pre-RFC:

# RFC

## Unsize- Feature Name: `unsizev2`
- Start Date: (fill me in with today's date, YYYY-MM-DD)
- RFC PR: [rust-lang/rfcs#0000](https://github.com/rust-lang/rfcs/pull/0000)
- Rust Issue: [rust-lang/rust#0000](https://github.com/rust-lang/rust/issues/0000)

# Summary
[summary]: #summary

Move the unsizing logic out of the compiler into library source, allowing for a more flexible design of the features.

# Motivation
[motivation]: #motivation

Currently unsizing in Rust is very rigid, only allowed in very specific scenarios permitted by the rules surrounding the current `Unsize` and `CoerceUnsized` traits and it's automatic implementations by the compiler.
Due to this rigidness of the current traits, it is also not uncommon to see types implement `Deref` instead `Unsize` to get deref coercions instead of unsizing coercions which can happen in the same places (and more) as unsizing even if the type is semantically not a pointer.
This RFC attempts to make these rules more flexible by also allowing user implementations of the traits.

TODO: Flesh this out, guide level explanation also needs more examples

# Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

## Unsize

Unsizing relationships between two types can be defined by implementing one of the three unsizing traits `Unsize`, `StableUnsize` or `FromMetadataUnsize` for a type and its target unsized type.
These implementations describe how the unsizing has to be performed by specifying what the resulting metadata value of the unsized type is, as well as optionally specifying the address to the unsized object.
Depending on the need of the unsizing relationship, either of three traits can be used with `FromMetadataUnsize` implying `StableUnsize` and `StableUnsize` implying `Unsize`.

### FromMetadataUnsize

`FromMetadataUnsize` is used, if the metadata solely comes from the metadata of the to be unsized object (or from compile time information) and does not need to be extracted from the data of object being unsized.

An example for this type of unsizing is `[T; N]` to `[T]` unsizing, as the length metadata is encoded in the array's type:

```rs
// SAFETY: `Unsize::target_metadata` returns the same value as `FromMetadataUnsize::TARGET_METADATA`
unsafe impl<T, const N: usize> FromMetadataUnsize<[T]> for [T; N] {
    fn target_metadata((): <Self as Pointee>::Metadata) -> <[T] as Pointee>::Metadata {
        N
    }
}
```

### StableUnsize

`StableUnsize` is used, if the metadata comes from the object that is being unsized without changing the address of the object.
An example for this type of unsizing is trait upcasting which potentially requires reading from the vtable of the original object:
```rs
trait Super {}
trait Sub: Super {}
// SAFETY: The metadata returned by `target_metadata` is valid metadata for the resulting trait object.
unsafe impl StableUnsize<dyn Super> for dyn Sub {
    unsafe fn target_metadata(
        self: *const Self,
    ) -> <dyn Super as core::ptr::Pointee>::Metadata {
        // construction of the relevant metadata here
    }
}
```

### Unsize

`Unsize` is used, if the metadata comes from the object that is being unsized while also requiring to change the address of the object.
An example for this type of unsizing is `Vec<T>` to `[T]` unsizing, which redirects the pointer to the contained allocation within:

```rs
// SAFETY: The metadata returned by `target_metadata` belongs to the slice pointed to by the pointer returned by `target_address`.
unsafe impl<T> Unsize<[T]> for Vec<T> {
    unsafe fn target_metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        // SAFETY: self is a valid pointer per calling contract
        unsafe { (*self).len }
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        // SAFETY: self is a valid pointer per calling contract
        unsafe { (*self).buf.ptr.as_ptr().cast() }
    }
}
```

## CoerceUnsized

Unsizing coercions between two pointer types can be defined by implementing the `CoerceUnsized` trait for them.
Such a coercion is done by the compiler implicitly when required by inserting a call to the `CoerceUnsized::coerce_unsized` function.

A `CoerceUnsized` implementation has specific requirements to be valid which boil down to 2 kinds:

### A delegating `CoerceUnsized` impl

Such an impl is used for wrapper like types, such as `Cell<T>` or `Pin<T>` where the impl is required to list a `CoereceUnsized` bound on the generic parameters of the wrapping type.
An example impl for the `Cell<T>` type would be the following:
```rs
impl<T, U> CoerceUnsized<Cell<U>> for Cell<T>
where
    T: CoerceUnsized<U>
{
    fn coerce_unsized(self) -> Cell<U> {
        Cell::new(self.into_inner().coerce_unsized())
    }
}
```


### A non-delegating `CoerceUnsized` impl

Such an impl is used for actual pointer like types, such as `&'a T` or `Arc<T>`.
These kinds of impls are required to list a trait bound with one of the unsize traits on the generic parameters of the pointer types.
An example impl for the `& 'a T` type would be the following:
```rs
impl<'a, 'b, T, U> CoerceUnsized<&'a U> for &'b T
where
    'b: 'a,
    T: Unsize<U> + ?Sized,
    U: ?Sized
{
    fn coerce_unsized(self) -> &'a U {
        // SAFETY: [`Unsize`] demands that the return values of `Unsize::target_data_address` and `Unsize::target_metadata` make up a valid unsized object
        unsafe {
            &*ptr::from_raw_parts(
                // SAFETY: self is a reference and hence a valid raw pointer
                Unsize::target_data_address(self),
                // SAFETY: self is a reference and hence a valid raw pointer
                Unsize::target_metadata(self),
            )
        }
    }
}
```

We can make use of the most permissive trait `Unsize` for the bound here as our references is a simple borrowed pointer, so the target address changing causes us no harm and we are free to read from the address to extract the metadata at runtime.

An example impl for the `Arc<T>` type would be the following:
```rs
impl<T, U> CoerceUnsized<Arc<U>> for Arc<T>
where
    T: ?Sized + StableUnsize<U>,
    U: ?Sized
{
    fn coerce_unsized(self) -> Arc<U> {
        let ptr = Arc::into_raw(self);
        // SAFETY: The arc is safe to be constructed from the result as the metadata belongs to the original
        // pointer according to StableUnsize requirements
        unsafe {
            Arc::from_raw(ptr::from_raw_parts(
                ptr.cast(),
                // SAFETY: ptr is derived from a live Arc and is therefor a valid raw pointer
                Unsize::target_metadata(ptr),
            ))
        }
    }
}
```

Here we actually are required to bound this implementation with `StableUnsize` (or `FromMetadataUnsize`), as the target address may not change.
The reason for that is that the `Arc` owns the allocation but also because it puts the ref counts at a certain offset inside of the allocation, so a changing address would make it impossible to correctly touch up on those anymore.

These new definitions allow some more implementations of `CoerceUnsized` which were not previously possible, an example would be the following:
```rs
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
```
With such an impl, `Option<&[T; N]>` could coerce to `Option<&[T]>`.

# Reference-level explanation
[reference-level-explanation]: #reference-level-explanation


## `Unsize` changes

The `Unsize` trait is being split into three unsafe traits forming a hierarchy:


### `Unsize`

The new `Unsize` trait definition looks like the following:

```rs
/// # Safety
///
/// - The implementation of [`Unsize::target_metadata`] must return metadata that is valid for
/// the object pointed to by the output of [`Unsize::target_data_address`].
/// - The implementation of [`Unsize::target_data_address`] must return a pointer that is valid for `Target`
pub unsafe trait Unsize<Target>
where
    Target: ?Sized,
{
    /// # Safety
    ///
    /// `self` must be a valid pointer to an instance of `Self`
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata;
    /// # Safety
    ///
    /// `self` must be a valid pointer to an instance of `Self`
    unsafe fn target_data_address(self: *const Self) -> *const ();
}
```

This trait forms the top of the hierarchy of the 3 unsizing traits.
Implementors of this trait can freely extract the metadata and address from the object that is being unsized.


### `StableUnsize`

A second unsizing trait called `StableUnsize` is introduced for unsizing relationships that keep the address of the object the same.

```rs
/// Same as [`Unsize`] but the target data address may not change.
///
/// # Safety
///
/// - The implementation of [`StableUnsize::target_metadata`] must return metadata that is valid for
/// the object pointed to by the `self` parameter
/// - The `Self` type and [`Target`] type must be layout compatible.
pub unsafe trait StableUnsize<Target>: Unsize<Target>
where
    Target: ?Sized,
{
    /// # Safety
    ///
    /// `self` must be a valid pointer to an instance of `Self`
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata;
}
```

This trait is more restrictive than `Unsize` by requiring the target address to not change.


### `FromMetadataUnsize`

```rs
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

```

This trait is the most restrictive of the three by requiring the unsizing to happen solely through compile time information.

## `CoerceUnsized`

The new `CoerceUnsized` trait definition looks like the following:

```rust
pub trait CoerceUnsized<Target> {
    fn coerce_unsized(self) -> Target;
}
```

Implementations of this trait now specifiy how the coercion is done.
This also drops the `?Sized` bound on `Target`, as returning unsized values is not possible.

In order to prevent misuse of the trait as means of implicit conversions, implementations for this trait require specific conditions to hold which the compiler will enforce.
For an implementation to be valid, one of the following must hold:
- Both `Self` and `Target` are references or raw pointers to differing generic parameters where the parameter `T` of `Self` has `T: UnsizeTrait<U>` bound with `U` being the generic parameter of `Target` and `UnsizeTrait` being one of the 3 unsize traits.
- `Self` and `Target` must have the same type constructor, and only vary in a single type parameter. The type parameter of `Self` must then have a `CoerceUnsized<U>` bound where `U` is the differing type parameter of `Target`. Example: `impl<T: CoerceUnsized<U>, U> CoerceUnsized<Cell<U>> for Cell<T>`
- `Self` and `Target` must have the same type constructor, and only vary in a single type parameter. The type parameter of `Self` must then have a `UnsizeTrait<U>` bound where `U` is the differing type parameter of `Target` and `UnsizeTrait` is one of the three unsize traits. Example: `impl<T: ?Sized + StableUnsize<U>, U: ?Sized, A: Allocator> CoerceUnsized<Box<U, A>> for Box<T, A> `

## Implementations provided by the standard library

For ergonomic purposes, the `core` library will provide the following blanket implementations:

`StableUnsize<T>` implies `Unsize<T>`.
```rs
// SAFETY: The metadata returned by `target_metadata` is valid metadata for the resulting trait object as per `StableUnsize::target_metadata` implementation
// and the implementing type and [`Target`] are layout compatible as per `Unsize::target_data_address` requirement.
unsafe impl<T, Target> Unsize<Target> for T
where
    Target: ?Sized,
    T: StableUnsize<Target> + ?Sized,
{
    unsafe fn target_metadata(self: *const Self) -> <Target as Pointee>::Metadata {
        // SAFETY: self is a valid raw pointer given the caller contract
        unsafe { <Self as StableUnsize<Target>>::target_metadata(self) }
    }

    unsafe fn target_data_address(self: *const Self) -> *const () {
        self.cast()
    }
}
```

This blanket impl removes the need of having to implement both traits and also protects from differing implementations.

`FromMetadataUnsize<T>` implies `StableUnsize<T>`.
```rs
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

```

Likewise, this blanket impl removes the need of having to implement all three traits and protects from differing implementations.


All current implementations provided by the standard library suite for today's `Unsize` trait will be reimplemented via `FromMetadataUnsize` accordingly.
Additionally, the compiler generated impl for `[T; N]: Unsize<[T]>` will be implemented in the `core` library instead as follows:

```rs
// SAFETY: `Unsize::target_metadata` returns the same value as `FromMetadataUnsize::TARGET_METADATA`
unsafe impl<T, const N: usize> FromMetadataUnsize<[T]> for [T; N] {
    fn target_metadata((): <Self as Pointee>::Metadata) -> <[T] as Pointee>::Metadata {
        N
    }
}
```

Additionally, 2 new unsizing implementations will be implemented for `Vec<T>` and `String` in the `alloc` crate:

```rs
// SAFETY: The metadata returned by `target_metadata` belongs to the slice pointed to by the pointer returned by `target_address`.
unsafe impl<T> Unsize<[T]> for Vec<T> {
    unsafe fn target_metadata(self: *const Self) -> <[T] as Pointee>::Metadata {
        // SAFETY: self is a reference and hence a valid raw pointer
        unsafe { (*self).len() }
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        // SAFETY: self is a reference and hence a valid raw pointer
        unsafe { (*self).as_ptr().cast() }
    }
}

// SAFETY: The metadata returned by `target_metadata` belongs to the str pointed to by the pointer returned by `target_address`.
unsafe impl Unsize<str> for String {
    unsafe fn target_metadata(self: *const Self) -> <str as Pointee>::Metadata {
        // SAFETY: self is a reference and hence a valid raw pointer
        unsafe { (*self).len() }
    }
    unsafe fn target_data_address(self: *const Self) -> *const () {
        // SAFETY: self is a reference and hence a valid raw pointer
        unsafe { (*self).as_ptr().cast() }
    }
}
```

All current implementations provided by the standard library suite for today's `CoerceUnsized` trait except for the listed following ones will be reimplemented with the new definition of the trait while bounded by the new `Unsize` trait, making them more permissive.

The implementations for `*const T`, `*mut T` and `NonNull<T>` will be bounded by `FromMetadataUnsize`, as their data pointer cannot be read from safely.
The implementation for `Box<T>`, `Rc<T>`, `Arc<T>`, `rc::Weak<T>` and `sync::Weak<T>` will be bounded by `StableUnsize`, as the pointer is effectively owned and cannot change.
The implementation for `Pin<T>` will be elaborated on in the later parts of this RFC.

## Implementations provided by the compiler

The compiler no longer provides an unsizing implementation for `[T; N]` as it is now provided by the core library.

The compiler will generate `FromMetadataUnsize` implementations for types to trait object for their implemented types (in fictional syntax):
```rs
unsafe impl<trait Trait, T: Trait> FromMetadataUnsize<dyn Trait> for T {
    fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <dyn Trait as Pointee>::Metadata {
        // magic
    }
}
```
These match today's trait object unsize implementations generated by the compiler.

The compiler will generate the following `FromMetadataUnsize` implementations for trait upcasting:
```rs
unsafe impl<trait Trait, trait Super> FromMetadataUnsize<dyn Super> for dyn Trait
where
    dyn Trait: Super
{
    unsafe fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <dyn super as Pointee>::Metadata {
        // compiler magic
    }
}
```
As trait upcasting is supported for raw pointers and it potentially requires vtable lookups, `FromMetadataUnsize` is required opposed to `Unsize` or `StableUnsize`, as we need to make sure not to dereference the actual data pointer part.

To keep backwards compatibility (as these are already observable in today's stable rust), the compiler also generates `FromMetadataUnsize<Foo<..., U, ...>>` implementations for structs `Foo<..., T, ...>` if all of these conditions are met:
- `T: FromMetadataUnsize<U>`.
- Only the last field of `Foo` has a type involving `T`.
- `Bar<T>: FromMetadataUnsize<Bar<U>>`, where `Bar<T>` stands for the actual type of that last field.
- `Foo<U>: Pointee<Metadata = <Bar<U> as Pointee>::Metadata>`


## DispatchFromDyn

`DispatchFromDyn` currently relies on `Unsize` for its bounds, these bounds can be changed to `FromMetadataUnsize` instead which will effectively not make any semantic changes to the trait.


## TypeMetadata<T> and Unsizing

See the following PR for context: https://github.com/rust-lang/rust/pull/97052.

With this new definition, we can implement `CoerceUnsized` for `TypeMetadata` without having to special case it in the compiler as follows:

```rs
struct TypedMetadata<T: ?Sized>(pub <T as core::ptr::Pointee>::Metadata);


impl<T, U> CoerceUnsized<TypedMetadata<U>> for TypedMetadata<T>
where
    T: ?Sized + FromMetadataUnsize<U>,
    U: ?Sized,
{
    fn coerce_unsized(self) -> TypedMetadata<U> {
        TypedMetadata(<T as FromMetadataUnsize<U>>::target_metadata(self.0))
    }
}
```

## Pin Unsoundness

See the following issue for context: https://github.com/rust-lang/rust/issues/68015

The design of the new traits here do not address the underlying issue in regards to `Pin`.
The author of this RFC feels like addressing the `Pin` soundness in the definitions of the traits is wrong, as in almost all cases where are a user implements one of these traits `Pin` will be irrelevant to them. And at its core, unsizing is not coupled with `Pin` whatsoever.
It would make much more sense to fix the unsound `CoerceUnsized` implementation that `Pin` provides.
That is given the current implementation of (with the new definition of the trait):
```rs

// Copied from core library docs:
// Note: this means that any impl of `CoerceUnsized` that allows coercing from
// a type that impls `Deref<Target=impl !Unpin>` to a type that impls
// `Deref<Target=Unpin>` is unsound. Any such impl would probably be unsound
// for other reasons, though, so we just need to take care not to allow such
// impls to land in std.
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P>
where
    P: CoerceUnsized<U>,
    // U: core::ops::Deref, this bound is accidentally missing upstream, hence we can't use `Pin::new_unchecked` in the implementation
{
    fn coerce_unsized(self) -> Pin<U> {
        Pin {
            pointer: self.pointer.coerce_unsized(),
        }
    }
}
```

We should rather strife to have the following 2 implementations:
```rs
// Permit going from Pin<impl Unpin> to Pin<impl Unpin>
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P>
where
    P: CoerceUnsized<U>,
    P: Deref<Target: Unpin>,
    U: Deref<Target: Unpin>,
{
    fn coerce_unsized(self) -> Pin<U> {
        Pin::new(self.pointer.coerce_unsized())
    }
}
// Permit going from Pin<impl Pin> to Pin<impl Pin>
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P>
where
    P: CoerceUnsized<U>,
    P: core::ops::Deref<Target: !Unpin>,
    U: core::ops::Deref<Target: !Unpin>,
{
    fn coerce_unsized(self) -> Pin<U> {
        // SAFETY: The new unpin Pin is derived from another unpin Pin, so we the pinned contract is kept up
        unsafe { Pin::new_unchecked(self.pointer.coerce_unsized()) }
    }
}
```


While this is a breaking change, it should be in line with being a soundness fix.
Unfortunately, these kind of impl requires negative bounds and negative reasoning which is its own can of worms, see https://github.com/rust-lang/rust/issues/42721.
Though maybe allowing them for auto traits alone could work out fine, given those types of traits are already rather special.

## Custom Reborrows

See the following issue for context: https://github.com/rust-lang/rfcs/issues/1403

In the linked issue the idea was to generalize `CoerceUnsized` as a general `Coerce` trait.
The author of this RFC believes that to be a bad call given the new design of the traits, as they now invoke user code with reborrows supposed to being no-ops.

# Drawbacks
[drawbacks]: #drawbacks

This proposal obviously makes unsizing more complex by introducing multiple traits for the concept.
It might also allow for some non-sensical implementations, though the restrictions on the `CoerceUnsized` trait try to limit them.
Unsizing coercions are now able to run arbitrary user code, placing it into a similar category to `Deref`.

# Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

- The design proposed here is (almost) maximally flexible, allowing most use cases to be covered at the expense of multiple unsizing traits by giving full control of how the unsizing happens. The `Unsize` trait hierarchy allows for bounding on certain requirements allowing safe implementations of `CoerceUnsized` with the new flexibilities in place.

- While `StableUnsize` is required for `Box` to be unsize coercible and `FromMetadataUnsize` is required for trait upcasting due to raw pointers allowing it, the general `Unsize` trait might not necessarily be needed, if use cases like `Vec<T>: Unsize<[T]>` are deemed unnecessary. This would simplify the proposal significantly.

- Alternatively, the design could be limited to the proposed `FromMetadataUnsize`, which is effectively today's `Unsize` trait with the addition of the `target_metadata` function. Doing so would still allow users to implement the trait and specifying how to derive the metadata from the source metadata opposed to having the compiler hardcode certain implementations of the trait. Introducing the other traits would then still be an option for the future, as they can be added as a supertrait of this trait with a corresponding blanket impl to prevent breakage.

# Prior art
[prior-art]: #prior-art

There is another Pre-RFC that tries to improve Unsizing which can be found [here](https://internals.rust-lang.org/t/pre-rfc-improved-unsizing/16861).
It does so by just allowing more impls of the current traits, while restricting them by taking visibilities of fields into account which complicates the traits in a confusing way.
And while the RFC makes  the traits more flexible, it does not offer the same flexibility that this proposal offers.


# Unresolved questions
[unresolved-questions]: #unresolved-questions
- Is the data address being allowed to change in unsizing coercions desirable?
- Given the pin unsoundness proposal, assuming negative reason was a thing, would an impl permitting to go from `Pin<impl Unpin>` to `Pin<impl !Unpin>` be safe?
- The compiler emitted implementations for the unsize traits, in particular the `Foo<..., T, ...>` case may collide with user implementations. Is this problematic? Should they be overridable?
- How does this RFC interact with `DispatchFromDyn`, does the trait need adjustments? Does this new design prevent `DispatchFromDyn` from evolving in certain ways?


# Future possibilities
[future-possibilities]: #future-possibilities

- Allow unsizing to take part in "autoderef", nowadays we do a single unsize step at the end of the autoderef chain for fixed length arrays. Assuming `Vec` and `String` should've been unsizable instead of dereferencable, this should've been done for ergonomics. (Though care would have to be taken to now allow this step to unsize to trait objects)
- Introduce more "delegating" `CoerceUnsized` implementations for standard library type, similar to the one for `Cell<T>`.
