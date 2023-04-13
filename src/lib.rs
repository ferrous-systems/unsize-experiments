#![no_std]
#![allow(incomplete_features)]
#![forbid(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]
#![feature(
    allocator_api,
    arbitrary_self_types,
    ptr_metadata,
    trait_upcasting,
    unsafe_pin_internals
)]

extern crate alloc;

pub mod coerce_unsized;
pub mod dispatch_from_dyn;
pub mod pointer;
pub mod unsize;

#[cfg(test)]
mod tests;

// https://github.com/rust-lang/rust/pull/97052
struct TypedMetadata<T: ?Sized>(pub <T as core::ptr::Pointee>::Metadata);
