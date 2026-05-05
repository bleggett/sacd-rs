# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/bleggett/sacd-rs/releases/tag/v0.1.0) - 2026-05-05

### Fixed

- fixups

### Other

- LFS these
- Use external dst-decoder crate
- Fix bug where consecutive small frames might erroneously be skipped
- Fix spot where we did not follow the spec for bounds checking
- Drop buffer pool, it's useless
- Optimize DST loop indexing (~4% gain)
- separate
- dst tests
- Add `nopad` flag from C impl
- Use a 3-thread model to decouple reading from writing
- Use SIMD for DST decode
- fmt
- Constify some bits
- Fixups + lints
- Use per-track threadpool + cleanups
- Fix progress bar to track frames written
- Split reader + decoder, and use C-style frame-based threaded decoding
- Move DST decoder
- DST works!
- cleanup
- reorg types
- start ISO extract
- Fixups, mch toc
- Dump info
- ISRC and times
- master text print as well
- add info print
- Add dump binary
- Dump fixes
- Raw ISO dumping
- Fixup area text
- like this
- area toc types/reorg
- toc parsing
- Move bits
- README
- Initial hack commit
- Initial commit
