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

There is actually another impl the compiler emits which is used for trait upcasting (unstable feature), interestingly though, this one we cannot replace with `ConstUnsize` as upcasting might need to access the vtable of the subtrait! So here we have to emit an impl that uses `StableUnsize`.

These compiler provided implementations have very rought impl skeletons written in pseudorust at the end of the [unsize.rs](src/unsize.rs) file.

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

### Unsoundness in regards to Pin

Today `Pin` has the following `CoerceUnsized` bound for `Pin`:
```rs
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P> where P: CoerceUnsized<U> {}
```
(Note the missing `U: Deref` bound which I would consider a bug)
As it turns out, this impl exposes unsoundness as this is effectively exposing `Pin::new_unchecked` without being unsafe itself!
The unsafety here lies in being able to coerce from a type that impls `Deref<Target=impl !Unpin>` to a type that impls `Deref<Target=impl Unpin>`, effectively enabling to violate the safety contract of the original pinned type in a fully safe context.

There are a few ways this could be fixed, sourced from https://internals.rust-lang.org/t/unsoundness-in-pin/11311/117

> Make `CoerceUnsized` an unsafe trait

To me personally, this feels like the wrong way to tackle it, as now the trait implementor has to be wary of `Pin` (which is notoriously hard to grasp for people), a concept that is more often than not irrelevant to people.

> Add an unsafe marker subtrait for `CoerceUnsized` which controls specifically whether a pinned version of that pointer can be coerced

This would effectively add bounds to the `CoerceUnsized` impl, resulting in something like
```rs
unsafe trait PinCoerceUnsized<Target>: CoerceUnsized<Target> {}
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P> where P: PinCoerceUnsized<U> {}
```
This "fixes" the unsound `CoerceUnsized` impl, but now pointer that wants `Pin` to be coercible for it has to instead impl this unsafe marker trait (or both) which pretty much boils down to be the same as solution #1 with extra steps

> Change nothing about `CoerceUnsized` but add an unsafe marker trait bound on `Pin::new` and require types that implement that marker + `CoerceUnsized` to be valid to coerce pinned.

This one I don't really understand honestly (as in where we should check what), and it also seems like an easy breaking change to add additional bounds to `Pin::new`

Another alternative that I would personally prefer which would require negative bound reasoning is changing the `CoerceUnsized` impl to the following two (with this repos definition of `CoerceUnsized`):
```rs
// Unpin -> Unpin
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
// Pin -> Pin
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P>
where
    P: CoerceUnsized<U>,
    P: Deref<Target: !Unpin>,
    U: Deref<Target: !Unpin>,
{
    fn coerce_unsized(self) -> Pin<U> {
        // SAFETY: The new unpin Pin is derived from another unpin Pin, so we the pinned contract is kept up
        unsafe { Pin::new_unchecked(self.pointer.coerce_unsized()) }
    }
}
```
with this we effectively fix the problematic conversions from being valid without having to introduce any pinning guarantees to `CoerceUnsized` itself.
Though unfortunately negative trait bounds themselves come with a bunch of problems, though maybe they can be limited to auto traits. https://github.com/rust-lang/rust/issues/42721

## DispatchFromDyn

`DispatchFromDyn` is currently bound by `Unsize`, and like `CoerceUnsized`, the actual "resizing" (that is going from the unsized trait object type to the concrete sized type) logic is builtin into the compiler.
Does the interface of `Unsize` have an effect on this definition?
Will this trait only be applicable to trait object types or to custom DSTs as well?

## Custom Reborrowable Types

See https://github.com/rust-lang/rfcs/issues/1403#issuecomment-166980781

The idea here is to generalize `CoerceUnsized` as just `Coerce`, modeling more of the magic coercions that the type system currently does via traits.
With this we could potential model reborrows by having impls of the trait that only differ in their lifetimes and teach borrowck that these should acts re-borrows.
I feel like with the proposal here invoking actual user functions though, this might be a rather bad call and instead we should look into creating a separate trait that marks reborrows.

## `dyn_star` trait upcasting

See https://github.com/rust-lang/rust/issues/104800
