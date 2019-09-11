use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;

// This is a self-referential struct because the slice field points to the data field.
// We cannot inform the compiler about that with a normal reference,
// as this pattern cannot be described with the usual borrowing rules.
// Instead we use a raw pointer, though one which is known not to be null,
// as we know it's pointing at the string.
#[derive(Debug)]
struct Unmovable {
    data: String,
    slice: NonNull<String>,
    _pin: PhantomPinned,
}

impl Unmovable {
    // To ensure the data doesn't move when the function returns,
    // we place it in the heap where it will stay for the lifetime of the object,
    // and the only way to access it would be through a pointer to it.
    fn new(data: String) -> Pin<Box<Unmovable>> {
        let res = Unmovable {
            data,
            // we only create the pointer once the data is in place
            // otherwise it will have already moved before we even started
            slice: NonNull::dangling(),
            _pin: PhantomPinned,
        };

        let mut boxed: Pin<Box<Unmovable>> = Box::pin(res);

        let slice: NonNull<String> = NonNull::from(&boxed.data);
        // we know this is safe because modifying a field doesn't move the whole struct
        // so assign `ptr` to our `boxed.slice`
        unsafe {
            let mut_ref: Pin<&mut Unmovable> = Pin::as_mut(&mut boxed);
            let unchecked: &mut Unmovable = Pin::get_unchecked_mut(mut_ref);
            unchecked.slice = slice;
        }
        boxed
    }
}

fn main() {
    let unmovable = Unmovable::new("Hello".to_string());
    // The pointer should point to the correct location,
    // so long as the struct hasn't moved.
    // Meanwhile, we are free to move the pointer around.
    println!("original = {:?}", &unmovable);
    let still_unmoved = unmovable;
    assert_eq!(still_unmoved.slice, NonNull::from(&still_unmoved.data));
    println!("moved    = {:?}", &still_unmoved);

    let a = "1".as_bytes().to_vec();
}
