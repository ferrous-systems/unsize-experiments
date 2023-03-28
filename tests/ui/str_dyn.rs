#![feature(ptr_metadata)]

trait Trait {
    fn dispatch_me(&self);
}

impl Trait for str {
    fn dispatch_me(&self) {}
}

unsafe impl unsizing_experiments::unsize::ConstUnsize<dyn Trait> for str {
    const TARGET_METADATA: <dyn Trait as core::ptr::Pointee>::Metadata = unimplemented!();
}

fn main() {
    let s: &dyn Trait = "".coerce_unsized();
}
