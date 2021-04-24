## Workspace setup
```shell
# used for project build
cargo install --force cargo-make

# required to get test code coverage working in CLion
cargo install --force grcov
```

## How To Build Project 
- [cargo-make][1] is used to manage the project builds

## NEAR References
- [staking-pool][4]

## References
- [toml][2]
- [cargo-make docs][3]

## Articles
- [composition over inheritance][5]

## Tools
- https://regexr.com/

[1]: https://crates.io/crates/cargo-make
[2]: https://toml.io/en/v1.0.0
[3]: https://sagiegurari.github.io/cargo-make
[4]: https://github.com/near/core-contracts/tree/master/staking-pool#staking-pool-contract-guarantees-and-invariants
[5]: https://www.reddit.com/r/rust/comments/372mqw/how_do_i_composition_over_inheritance/