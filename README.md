cargo-stress
============

A utility for catching non-deterministic test failures. It runs tests in
parallel in a loop and collects failures.

`cargo-stress` is modeled after the [stress] binary we use at [CockroachDB],
which is itself a fork of Go's [x/tools/cmd/stress].

`cargo stress` can be installed with `cargo install`. The resulting binary
should then be in `$HOME/.cargo/bin`.

```
$ cargo install cargo-stress
```

Use it to run your tests as fast as possible:
```
$ cargo stress
Compiling test binaries: cargo test --no-run --message-format=json
    Finished dev [unoptimized + debuginfo] target(s) in 0.01s
Running test binary: "cargo-stress/target/debug/cargo_stress-e02dca1c9609c365"
1 runs so far, 0 failures, over 0s
5092 runs so far, 0 failures, over 5s
10226 runs so far, 0 failures, over 10s
15455 runs so far, 0 failures, over 15s
...
```

## Flags and usage

Cargo is used to build the test binary, which is then invoked repeatedly. By
default, it runs the same tests as `cargo test` but this can be controlled via
flags. It is intended that any `cargo test` command can be replaced by `cargo
stress` and the same tests will be run with the same arguments. Any other
behavior is a bug. It's likely I didn't get this quite right, so please file
issues if you see this.

[stress]: http://github.com/cockroachdb/stress
[CockroachDB]: http://github.com/cockroachdb/cockroach
[x/tools/cmd/stress]: https://godoc.org/golang.org/x/tools/cmd/stress
