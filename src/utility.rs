use owo_colors::OwoColorize;

pub(crate) fn note() {
    eprint!("{}: ", "note".cyan());
}

pub(crate) fn default<T: Default>() -> T {
    T::default()
}

pub(crate) type SmallVec<T, const N: usize> = smallvec::SmallVec<[T; N]>;

pub(crate) trait Captures<'a> {}

impl<'a, T: ?Sized> Captures<'a> for T {}
