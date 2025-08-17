use bumpalo::Bump;

pub fn compose<'a, A, B, C>(
    mem: &'a Bump,
    f: &'a (dyn Fn(&'a Bump, B) -> C + 'a),
    g: &'a (dyn Fn(&'a Bump, A) -> B + 'a),
) -> &'a (dyn Fn(&'a Bump, A) -> C + 'a) {
    mem.alloc(|mem, val| f(mem, g(mem, val)))
}
