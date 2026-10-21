## Summary

<!-- What changes, in one or two sentences. -->

## Why

<!-- The problem this solves. Link the issue when there is one. -->

## Testing

- Model:
- Kernel:
- Commands run:

<!--
For hardware writes, prefer idempotent tests (write the value already in use) or a
round-trip of the firmware's own fan curve followed by `tufctl fan auto`.
-->

## Checklist

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Tested on hardware, or explained why not
- [ ] No new runtime dependencies
- [ ] README updated if behaviour or supported features changed
- [ ] No custom fan curve left enabled on the test machine
