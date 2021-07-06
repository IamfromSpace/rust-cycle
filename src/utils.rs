pub fn lift_a2_option<A, B, C, F: Fn(A, B) -> C>(a: Option<A>, b: Option<B>, f: F) -> Option<C> {
    match (a, b) {
        (Some(a), Some(b)) => Some(f(a, b)),
        _ => None,
    }
}

// Haskell's `sequence` as applied to nested optionals, which "flips" them.  If the outer is None,
// it moves it "inside" (to Some(None)) and if the inner is None it moves it "outside" (to None),
// and if both a present the result is simply the identity.
pub fn sequence_option_option<T>(a: Option<Option<T>>) -> Option<Option<T>> {
    match a {
        Some(x) => match x {
            Some(y) => Some(Some(y)),
            None => None,
        },
        None => Some(None),
    }
}
