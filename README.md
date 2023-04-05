# Unsize and CoerceUnsize v2

This repo experiments with a new design for the `Unsize` and `CoerceUnsized` traits, making them more flexible and applicable to more types.
The motivation is to see how flexible the traits could be made and whether this flexibility pays off or not.
An idea that kicked this off was to have `Vec<T>: Unsize<[T]>`, which requires user code to run to be able to adjust the pointer for the data.
Today `Vec<T>` implements `Deref<Target = [T]>` for convenience as this allows dispatching slice methods on objects of `Vec<T>`, ye `Deref` was meant for smart pointers instead. What this impl really feels like is more akin to unsizing. Obviously the `Deref` impl cannot be revoked from `Vec<T>` anymore, but assuming an unsizing impl would've been the proper call this would then also imply that unsizing would have to be introduced into autoderef (instead of just special casing array unsizing).

## Unsize

The `Unsize` trait has been split into 3 new traits: `ConstUnsize`, `StableUnsize` and `Unsize`.
The three traits form a hierarchy where `ConstUnsize` is the most specific and `Unsize` the least specific, i.e `ConstUnsize` requires `StableUnsize` requires `Unsize`.

### Unsize

The new `Unsize` trait now looks like

```rs
/// # Safety
///
/// - The implementation of [`Unsize::target_metadata`] must return metadata that is valid for
/// the object pointed to by the output of [`Unsize::target_data_address`].
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
    unsafe fn target_data_address(self: *const Self) -> *const ();
}
```

First up, this trait may now be implemented by users directly, while the compiler will still emit certain impls which will be outlined in below.
The biggest changes to the trait are the addition of the two functions `target_metadata` and `target_data_address`.
Both these functions receive a raw pointer to the object that is the target of the unsizing.
`target_metadata` is responsible for extracting the metadata for the unsized object from the sized one and `target_data_address` is responsible for retrieving a pointer to the unsized data.
`target_data_address` exists for allowing impls such as `Vec<T>: Unsize<[T]>`, as the unsizing operation here requires chasing a pointer into the vec.

This new definition allows custom unsizing to be very flexible (and very unsafe <NOTE DOWN WHY>).
Unfortunately this flexibility comes at a price, as this new definition requires a valid pointer to the object being unsized which is not always at hand (example being unsizing `TypedMetadata`).
Some impls also require the data pointer to not change when unsizing, example being the unsize impl for `Arc<T>` which relies on the fact that the ref counts are immediately before the data pointer.
And finally, for raw pointer coercions to take place, we are not allowed to dereference the pointers themselves at all (as unsize coercions are safe operations), so in this case we need to fetch the metadata solely from compile time information.

This introduces the need for two more traits which can uphold these invariants, the first of which is `StableUnsize` defined as:

```rs
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
```

Implementing `StableUnsize` will automatically implement `Unsize` (via a blanket impl) for the implementing type with a correct `target_data_address` implementation that keeps the addition invariant.

The second trait that is needed is `ConstUnsize` which is defined as:

```rs
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
```

Implementing `ConstUnsize` will automatically implement `StableUnsize` (via a blanket impl) for the implementing type with the `target_metadata` method delegating to the `TARGET_METADATA` const.

`ConstUnsize` effectively mimics today's `Unsize` trait, which seeds the metadata for unsizing from compile time information provided by the compilers generated implementations, as such the compiler generated implementations will be emitted for `ConstUnsize` specifically.

The compiler today emits 3 "kinds" of implementations for `Unsize`, which would be changed in the following way with this proposal:
- `[T; N]: Unsize<[T]>`: With `ConstUnsize`, this impl can now be written in source in the core library
- `T: Unsize<dyn Trait>` where `T: Trait`: The compiler will continue emitting these but for `ConstUnsize` instead with the metadata const filled appropriately.
- And lastly the most complex rule which needs to be kept for backwards compat reasons:
    Structs `Foo<..., T, ...>` implement `ConstUnsize<Foo<..., U, ...>>` if all of these conditions are met:
    - `T: ConstUnsize<U>`.
    - Only the last field of `Foo` has a type involving `T`.
    - `Bar<T>: ConstUnsize<Bar<U>>`, where `Bar<T>` stands for the actual type of that last field.

open questions:
    - Should it be allowed to change the target address, that is does `Unsize` make sense or is `StableUnsize` and `ConstUnsize` all that we need? Disallowing this would prevent `Vec<T>: [T]` as well as some other class of implementations (see for example the test `fixed_str_dyn_len` in [tests.rs](src/tests.rs))
    - Do the functions on the traits make sense as defined?
    - (the use of arbitrary self types for the self pointers was on a whim, whether we make them associated functions or not is not really too relevant)
    - ideally the `Target: ?Sized` bound should be `Target: !Sized`, that is effectively forbidding unsizing implementations to sized targets, but this is not expressible today. The compiler could easily reject those kinds of impls anyways though.

## CoerceUnsized

To make use of these new unsize traits, we also need to lift out the hard coded compiler logic for unsizing coercions into the `CoerceUnsized` trait itself.
The new definition therefor is:

```rs
pub trait CoerceUnsized<Target> {
    fn coerce_unsized(self) -> Target;
}
```

This effectively allows implementing the actual coercion logic in user code.
For example implementations of the majority of core/alloc/std implementations that exist/could exist take a look at [coerce_unsized.rs](src/coerce_unsized.rs).

Interesting to see there is that while most impls can make use of `Unsize` as the trait bounds, some require the more specific traits to work as was mentioned above, notably:
- `Arc<T>`/`Rc<T>`/`Box<T>` require `StableUnsize` as the are owning the allocation
- `TypeMetadata<T>` requires `ConstUnsize` as there is no actual object stored to unsize on
- `*const T`/`*mut T` require `ConstUnsize` as the pointers may not actually be valid, so we can't access the pointee to do the unsizing on


open questions:
    - Are there ways to make an implementation of this cause unsoundness? That is are there places where a certain behavior for unsizing coercions is being relied on that may be broken by this?
    - Look into the unsoundness with regards to `Pin`


## DispatchFromDyn

Is here in part due to some other experimentation exploring this trait's design space, not much progress here yet.


## General open questions

- Custom user borrows came up at some point where one of the ideas is to generalize the `CoerceUnsize` trait into something that also allows reborrowing, see https://github.com/rust-lang/rfcs/issues/1403#issuecomment-166980781
- `dyn_start` can't implement trait upcasting due to it being structural, see https://github.com/rust-lang/rust/issues/104800
- describe and integrate trait upcasting into the new definitions
