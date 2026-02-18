# Performance

## General Principles

- Profile first, optimize second - use `cargo flamegraph`
- Algorithm/data structure changes beat micro-optimizations
- Always benchmark with `--release` (debug is 10-100x slower)

## Memory

- Prefer `&T` (borrowing) over `T` (ownership) when possible
- Use `&str` over `String` in function parameters
- Avoid unnecessary `.clone()` - use references
- Consider `Cow<str>` when you sometimes need ownership

## Collections

- `Vec` is almost always the right choice (cache-friendly)
- `LinkedList` is rarely appropriate (10x+ slower than Vec)
- Pre-allocate with `Vec::with_capacity()` when size is known
- Use `VecDeque` for frequent front insertions/removals

## Iterators

```rust
// Good - zero-cost abstractions
collection.iter()
    .filter(|x| x.is_valid())
    .map(|x| x.transform())
    .collect()

// Avoid - bounds checking overhead
for i in 0..collection.len() {
    collection[i] // bounds check on each access
}
```

## Strings

- Use `&str` for read-only string data
- `String::with_capacity()` for building strings
- `format!` is convenient but allocates - avoid in hot paths
- Consider `write!` to a buffer for repeated formatting
