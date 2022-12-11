# Solana Anchor Lens
A deserializer for Anchor accounts and instructions on Solana!

Do you know how Solana block explorers will automatically parse 
and display account or instruction data for you whenever
there's an on-chain IDL?
This is that magic, contained in a nice, shiny Rust crate and CLI!

It does the job by:

1. Reading a Solana transaction or account data,
2. Looking up the on-chain IDL to its owned program(s), and
3. Using that IDL to deserialize the transaction or account data into a `serde_json::Value`.

### Features
- The Transaction deserialization will also validate the privilege escalations encoded
in the transaction against what's stipulated in the IDL. If skipping preflight checks,
this can be a useful analytical tool for debugging bad client-side transaction construction.
- Each step to the process is exposed for fine-grained control over how one chooses to integrate
this library. But there is also a convenience class that makes fetch and deserialize operations
more or less one-liners.
- IDLs can be internally cached to save on RPC calls.

## Examples
See the examples directory or run:

```
$ cargo run --example account
$ cargo run --example transaction
```

# TODO
- CLI crate
- Parse inner instructions
- CLI instruction -- Account Dump to JSON file from Deserialized.
- CLI instruction -- Accound Dump mint, modified
- CLI instruction -- Accound Dump token, modified
