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

Move the unsizing logic out of the compiler into library source, allowing for a more flexible design of the features and allowing user code to implement `Unsize` for their own types.

# Motivation
[motivation]: #motivation

Currently unsizing in Rust is very rigid, only allowed in very specific scenarios permitted by the rules surrounding the current `Unsize` and `CoerceUnsized` traits and its automatic implementations by the compiler.

This has the downside of being very magical, as the majority of the logic happens inside the compiler opposed to the source code.
It also prevents certain unsizing implementations from being doable today.

This RFC attempts to make these rules more flexible by also allowing user implementations of the traits that define how the metadata is derived and composed back into unsized objects.

# Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

## Unsize

Unsizing relationships between two types can be defined by implementing the unsafe `Unsize` trait for a type and its target unsized type.
These implementations describe how the metadata is derived from a source type and its metadata for its target unsized type.

An example implementation of `Unsize` for `[T; N]` to `[T]` unsizing looks like the following:

```rust
// SAFETY:
// - `Unsize::target_metadata` returns length metadata that spans the entire array exactly.
// - `[T; N]` is a contiguous slice of `T`'s, so a pointer pointing to its data is valid
//   to be interpreted as a pointer to a slice `[T]`.
unsafe impl<T, const N: usize> Unsize<[T]> for [T; N] {
    fn target_metadata((): <Self as Pointee>::Metadata) -> <[T] as Pointee>::Metadata {
        N
    }
}
```

The metadata for the source type `[T; N]` is the unit type `()`, as there is no metadata for sized types.
The implementation then just returns the length `N` from the array type, as this is the appropriate metadata for a slice produced from such an array.

<!-- not the best example thats following here, but I could not think of an unsized to unsized relationship thats not trait upcasting, given custom unsized typed are not currently a thing -->

An example that does an unsized to unsized coercion is the following implementation (for trait upcasting provided by the compiler):

```rust
trait Super {}

trait Sub: Super {}

// SAFETY:
// - `Unsize::target_metadata` returns a vtable provided by the vtable of the `dyn Sub` object.
// - `dyn Super` is a super trait of `dyn Sub`, so a pointer pointing to data for a `dyn Sub`
//   is valid to be used as a data pointer to a `dyn Super`
unsafe impl Unsize<dyn Super> for dyn Sub {
    unsafe fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <dyn super as Pointee>::Metadata {
        metadata.upcast()
    }
}
```

This is an unsizing impl required for trait upcasting, where the metadata (the vtable of the trait) of the `dyn Super` type has to be extracted from the the metadata of the `dyn Sub` trait.

## CoerceUnsized

To actually enable the unsizing coercion of objects, the `CoerceUnsized` trait has to be implemented.
It defines how the unsizing of the inner type occurs for a given pointer or wrapper type.

A `CoerceUnsized` implementation has specific requirements to be valid which boil down to 2 kinds:
1. A non-delegating `CoerceUnsized` impl
2. A delegating `CoerceUnsized` impl

### 1. A non-delegating `CoerceUnsized` impl

Such an impl is used for actual pointer like types, such as `&'a T` or `Arc<T>`.
The implementing type and the `CoerceUnsized` target type must differ in a single generic parameter only. Say, the parameters are `T` and `U`. Then,

- `T` is the generic paramter of the implementing type; is bound as `T: Unsize<U>`
- `U` is the generic paramter of the `CorecedUnsized` target type

#### Example impl for the `& 'a T` type

```rust
impl<'a, 'b, T, U> CoerceUnsized<&'a U> for &'b T
where
    'b: 'a,
    T: Unsize<U> + ?Sized,
    U: ?Sized
{
    fn coerce_unsized(self) -> &'a U {
        let metadata = Unsize::target_metadata(core::ptr::metadata(self));
        let untyped_data_ptr = (self as *const T).cast::<()>();
        // SAFETY: [`Unsize`] demands that the return value of
        // `Unsize::target_metadata` is valid to be used together
        // with the data pointer to be re-interpreted as the unsized type
        unsafe { &*core::ptr::from_raw_parts(untyped_data_ptr, metadata) }
    }
}
```

#### Example impl for the `Arc<T>` type
```rust
impl<T, U> CoerceUnsized<Arc<U>> for Arc<T>
where
    T: ?Sized + Unsize<U>,
    U: ?Sized
{
    fn coerce_unsized(self) -> Arc<U> {
        let ptr = Arc::into_raw(self);
        let metadata = Unsize::target_metadata(core::ptr::metadata(ptr));
        let untyped_data_ptr = (ptr as *const T).cast::<()>();
        // SAFETY: [`Unsize`] demands that the return value of
        // `Unsize::target_metadata` is valid to be used together
        // with the data pointer to be re-interpreted as the unsized type
        // and that `std::mem::size_of` on `U` will report the same size as the `T`.
        unsafe { Arc::from_raw(core::ptr::from_raw_parts(untyped_data_ptr, metadata)) }
    }
}
```

⚠️ **Important** to note is that `Unsize` impls are required to return metadata that make the unsized object report the same size as the source type. If that was not the case, the `Arc` impl above would be unsound, as its destructor would try to deallocate a smaller allocation than it initially owned.

### 2. A delegating `CoerceUnsized` impl

Such an impl is used for wrapper like types, such as `Cell<T>` or `Pin<T>` where the impl is required to list a `CoerceUnsized` bound on the generic parameters of the wrapping type.

#### Example impl for the `Cell<T>` type
```rust
impl<T, U> CoerceUnsized<Cell<U>> for Cell<T>
where
    T: CoerceUnsized<U>
{
    fn coerce_unsized(self) -> Cell<U> {
        Cell::new(self.into_inner().coerce_unsized())
    }
}
```

#### Example implementation for `Option<T>`
A delegating impl is not limited to `struct` types.
```rust
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


## `Unsize`

The new `Unsize` trait definition looks like the following:

```rust
/// # Safety
///
/// The implementation of [`Unsize::target_metadata`] must return metadata that
/// - is valid for interpreting the `Self` type to `Target`, and
/// - where using `core::mem::size_of` on the unsized object will report the
///   same size as on the source object.
pub unsafe trait Unsize<Target>
where
    Target: ?Sized,
{
    fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <Target as Pointee>::Metadata;
}
```

This trait allows specifying how to derive the metadata required for unsizing from the metadata of the source type `Self` or the compile time type information.

## `CoerceUnsized`

The new `CoerceUnsized` trait definition looks like the following:

```rust
pub trait CoerceUnsized<Target> {
    fn coerce_unsized(self) -> Target;
}
```

Implementations of this trait now specify how the coercion is done.
This also drops the `?Sized` bound on `Target`, as returning unsized values is not possible currently.
This can be relaxed without breakage in the future.

In order to prevent misuse of the trait as means of implicit conversions, implementations for this trait require specific conditions to hold which the compiler will enforce.
This can be relaxed without breakage in the future.

For an implementation to be valid, one of the following must hold:
1. `Self` and `Target`
    - must be references or raw pointers to different generic parameters
    - type parameter `T` of `Self` has `T: Unsize<U>` bound where `U` is the type parameter of `Target`
2. `Self` and `Target`
    - must have the same type constructor, varying in a single type parameter
    - type parameter `T` of `Self` must have a `T: CoerceUnsized<U>` bound where `U` is the type parameter of `Target`
    - Example:
        ```rust
        impl<T: CoerceUnsized<U>, U> CoerceUnsized<Cell<U>>
            for Cell<T>
        ```
3. `Self` and `Target`
    - must have the same type constructor, varying in a single type parameter
    - type parameter `T` of `Self` must have a `T: Unsize<U>` bound where `U` is the differing type parameter of `Target`
    - Example:
        ```rust
        impl<T: ?Sized + Unsize<U>, U: ?Sized, A: Allocator> CoerceUnsized<Box<U, A>>
            for Box<T, A>
        ```

## Implementations provided by the standard library

### `Unsize`

Today, all `Unsize` implementations are provided by the compiler.
Most of them will continue to be provided by the compiler as they involve trait objects which depend on all traits defined.
The only one that will no longer be emitted by the compiler is the `[T; N]: Unsize<[T]>` implementation as we can now fully implement it in library source.
The implementation will be as follows (and live in `core`):

```rust
// SAFETY:
// - `Unsize::target_metadata` returns length metadata that spans the entire array exactly.
// - `[T; N]` is a contiguous slice of `T`'s, so a pointer pointing to its data is valid to
//   be interpreted as a pointer to a slice `[T]`.
unsafe impl<T, const N: usize> Unsize<[T]> for [T; N] {
    fn target_metadata((): <Self as Pointee>::Metadata) -> <[T] as Pointee>::Metadata {
        N
    }
}
```

### `CoercedUnsized`

The non-delegating implementations of `CoerceUnsized` provided by the standard library will have the implementation of their `fn coerce_unsized` function written to disassemble the source into pointer and source metadata, make use of the `Unsize` trait for extracting the target metadata from the source metadata, and then reassembling the pointer and target metadata into the target.

For the delegating implementations, the implementation of the `fn coerce_unsized` function will merely delegate to the inner value and then wrap that result again.

## Implementations provided by the compiler

> ⚠️ Note: This section uses fictional rust syntax

The compiler will generate `Unsize` implementations for types to trait object for their implemented types as before:

For types to trait object for their implemented types, the compiler will generate `Unsize` implmentations:
```rust
unsafe impl<trait Trait, T: Trait> Unsize<dyn Trait> for T {
    fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <dyn Trait as Pointee>::Metadata {
        // magic
    }
}
```

For the unstable trait upcasting feature, the compiler will generate the following `Unsize` implementations:
```rust
unsafe impl<trait Trait, trait Super> Unsize<dyn Super> for dyn Trait
where
    dyn Trait: Super
{
    unsafe fn target_metadata(metadata: <Self as Pointee>::Metadata) -> <dyn super as Pointee>::Metadata {
        // compiler magic
    }
}
```
This is safe to do, as the metadata of source can be safely extracted from a raw pointer without touching the data for a `CoerceUnsized` implementation on `*const T`/`*mut T`.


To keep backwards compatibility (as these are already observable in today's stable rust), the compiler also generates `Unsize<Foo<..., U, ...>>` implementations for structs `Foo<..., T, ...>` if all of these conditions are met:
- `T: Unsize<U>`.
- Only the last field of `Foo` has a type involving `T`.
- `Bar<T>: Unsize<Bar<U>>`, where `Bar<T>` stands for the actual type of that last field.
- `Foo<U>: Pointee<Metadata = <Bar<U> as Pointee>::Metadata>`

## Unsize un-lowering for known impls

The compiler may "un-lower" some known unsize coercions back into builtin operations in the MIR as to not degrade performance too much, as lowering this new definition will introduce a lot of new operations that don't exist in the current unsizing logic.
This would be similar to how builtin operators for primitives work currently, where they are typechecked with the trait impls but then lowered back to builtin operators in the mir.

## `TypeMetadata<T>` and Unsizing

See the following PR for context: [Implement pointee metadata unsizing via a `TypedMetadata<T>` container #97052](https://github.com/rust-lang/rust/pull/97052).

With this new definition, we can implement `CoerceUnsized` for `TypeMetadata` without having to special case it in the compiler as follows:

```rust
struct TypedMetadata<T: ?Sized>(pub <T as core::ptr::Pointee>::Metadata);


impl<T, U> CoerceUnsized<TypedMetadata<U>> for TypedMetadata<T>
where
    T: ?Sized + Unsize<U>,
    U: ?Sized,
{
    fn coerce_unsized(self) -> TypedMetadata<U> {
        TypedMetadata(Unsize::target_metadata(self.0))
    }
}
```

## Pin Unsoundness

See the following issue for context: [`Pin` is unsound due to transitive effects of `CoerceUnsized` #68015
](https://github.com/rust-lang/rust/issues/68015)

The design of the new traits here do not address the underlying issue in regards to `Pin`.
The author of this RFC feels like addressing the `Pin` soundness in the definitions of the traits is wrong, as in almost all cases where are a user implements one of these traits `Pin` will be irrelevant to them.
And at its core, unsizing is not coupled with `Pin` whatsoever.

It would make much more sense to fix the unsound `CoerceUnsized` implementation that `Pin` provides.

That is given the current implementation of (with the new definition of the trait):
```rust

// Copied from core library docs:
// Note: this means that any impl of `CoerceUnsized` that allows coercing from
// a type that impls `Deref<Target=impl !Unpin>` to a type that impls
// `Deref<Target=Unpin>` is unsound. Any such impl would probably be unsound
// for other reasons, though, so we just need to take care not to allow such
// impls to land in std.
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P>
where
    P: CoerceUnsized<U>,
    // `U: core::ops::Deref`, this bound is accidentally missing upstream,
    // hence we can't use `Pin::new_unchecked` in the implementation
{
    fn coerce_unsized(self) -> Pin<U> {
        Pin {
            pointer: self.pointer.coerce_unsized(),
        }
    }
}
```

Instead, we should rather strive to have the following 2 implementations:
```rust
// Permit going from `Pin<impl Unpin>` to` Pin<impl Unpin>`
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

// Permit going from `Pin<impl Pin>` to `Pin<impl Pin>`
impl<P, U> CoerceUnsized<Pin<U>> for Pin<P>
where
    P: CoerceUnsized<U>,
    P: core::ops::Deref<Target: !Unpin>,
    U: core::ops::Deref<Target: !Unpin>,
{
    fn coerce_unsized(self) -> Pin<U> {
        // SAFETY: The new unpin Pin is derived from another unpin Pin,
        // so we the pinned contract is kept up
        unsafe { Pin::new_unchecked(self.pointer.coerce_unsized()) }
    }
}
```


While this is a breaking change, it should be in line with being a soundness fix.
Unfortunately, these kind of impl requires negative bounds and negative reasoning which is its own can of worms and therefore likely not to happen, see [GH issue: Need negative trait bound #42721](https://github.com/rust-lang/rust/issues/42721).
Though maybe allowing them for auto traits alone could work out fine, given those types of traits are already rather special.

Assuming this path would be blessed as the future fix for the issue, this RFC itself will not change the status quo of the unsoundness and therefore would not need to be blocked on negative bounds/reasoning.

## Custom Reborrows

See the following issue for context: [Some way to simulate `&mut` reborrows in user code #1403](https://github.com/rust-lang/rfcs/issues/1403)

In the linked issue the idea was to generalize `CoerceUnsized` as a general `Coerce` trait.

The design proposed here would still enable this, although it raises some questions.
- For one, reborrows should be guaranteed to be no-ops, as all that should change in "reborrow coercions" are the corresponding lifetimes, yet this RFC exposes a function that will be run on coercion.
- Generalizing this to a general `Coerce` trait would require specialization and/or negative trait bound reasoning, such that `&'a mut T: CoerceUnsized<&'b T>` (for reborrows) but also `&'a mut T: CoerceUnsized<&'b U>, T: Unsize<U>` for unsizing coercions can both be done as impls.

The first issue is only of concern with the proposed design here, while the second one is a more general issue relevant to impl overlap.

# Drawbacks
[drawbacks]: #drawbacks

- This proposal allows for some non-sensical `CoerceUnsized` implementations resulting in odd unsizing coercions (think implicit casts where no actual "unsizing" happens), though the restrictions on the `CoerceUnsized` trait try to limit them (for the time being).
  - This includes implementations that may allocate
- Unsizing coercions are now able to run arbitrary user code, placing it into a similar category to `Deref` in that regard, effectively adding yet more user facing \*magic\* to the language.
- The `Unsize` trait now depends on the `Pointee` trait which means any push for stabilization will depend on the stabilization of said trait.

# Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

- As was discussed in the custom reborrows issue, we could make `CoerceUnsized` represent a more general user controlled `Coerce` mechanism.
- This proposal is forwards compatible with exposing more dynamic unsizing behavior in the future, where for example the metadata is read from a field of the source type. To support that, a new trait `DynamicUnsize` could be introduced as the supertrait of `Unsize`, exposing the needed functions to extract the metadata. Then a blanket impl can be provided that implements `DynamicUnsize` for anything implementing `Unsize` with delegating the metadata extraction functions to the `Unsize` impl. The reason for why such a split would be necessary is that not all coercions can read from the source object (raw pointer unsizing for example), so there needs to be a way to differentiate on the trait bounds for the corresponding `CoerceUnsized` implementations.

# Prior art
[prior-art]: #prior-art

There is [another Pre-RFC that tries to improve Unsizing](https://internals.rust-lang.org/t/pre-rfc-improved-unsizing/16861).
It does so by just allowing more impls of the current traits, while restricting them by taking visibilities of fields into account which complicates the traits in a (subjectively to the author) confusing way.
And while the RFC makes the traits more flexible, it does not offer the same flexibility that this proposal offers.


# Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Given the `Pin` unsoundness proposal, assuming negative reason was a thing, would an impl permitting to go from `Pin<impl Unpin>` to `Pin<impl !Unpin>` be sound?
2. The compiler emitted implementations for the unsize trait, in particular the `Foo<..., T, ...>` case may collide with user implementations. 
    1. Is this problematic?
    2. Should they be overridable?
3. Will this design prevent any scenarios from being ever supported?
4. As usual, naming. Given we might want to introduce multiple unsize traits for certain requirements, should the proposed trait stick to `Unsize` or something more specific like `FromMetadataUnsize`?

# Future possibilities
[future-possibilities]: #future-possibilities

- Expand the compiler emitted implementations of `CoerceUnsized` to enums, such as `Option<T>: CoerceUnsized<Option<U>>` where `T: CoerceUnsized<U>`.
- Add a `DynamicUnsize` trait as outlined in the rationale to support more unsizing use cases.
