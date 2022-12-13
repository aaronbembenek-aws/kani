# Firecracker Stubbing Demo

This package demonstrates how to use Kani with function stubbing to prove a property about a function from [Firecracker](https://firecracker-microvm.github.io/), an open-source virtual machine monitor.

The function in question is [`parse_put_vsock`](https://github.com/firecracker-microvm/firecracker/blob/41a386b2e0317affc04474d2ca04c280bde84445/src/api_server/src/request/vsock.rs#L11-L34), which parses some data into a request.
The harness `demo_harness` tries to prove the property that this function returns parsing metadata containing a deprecation message if and only if it parsed a virtual socket device configuration containing a virtual socket ID.

To run the harness, use the command:

```
cargo kani --enable-unstable --enable-stubbing --harness demo_harness
```

On my laptop, this completes in about 10 seconds (including compilation time), using Kani v0.16.0 and CBMC v5.71.0.

## Caveats

- I've commented out a couple `METRICS` statements in `parse_put_vsock`.
  The proof still goes through with them uncommented, but takes much longer (~8 minutes).
  Interestingly, stubbing them out with no-ops does not seem to help.
  - In the past, I've had better performance using `kissat` as the backend SAT solver when trying to verify the unmodified function, using this command: 
    ```
    cargo kani --enable-unstable --enable-stubbing --harness demo_harness --cbmc-args --external-sat-solver /path/to/kissat
    ```
    However, external solvers seem to be triggering a bug in more recent versions of Kani/CBMC (tracked in [this issue](https://github.com/model-checking/kani/issues/1962)).
  - **Update:** The proof runs quite quickly (20s) with all the `METRICS` statements included if you use CBMC built with CaDiCaL (see [build instructions](https://github.com/model-checking/kani/issues/1962#issuecomment-1345374290)).
- To cut down on the number of dependencies to pull into a single file, I've simplified some of the Firecracker data structures (i.e., removed some unused fields and variants).
  This could affect verification times as well.