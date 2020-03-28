pub fn lift_a2_option<A, B, C, F: Fn(A, B) -> C>(a: Option<A>, b: Option<B>, f: F) -> Option<C> {
    match (a, b) {
        (Some(a), Some(b)) => Some(f(a, b)),
        _ => None,
    }
}
