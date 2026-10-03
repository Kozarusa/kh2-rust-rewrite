# OpenKH

- Project: https://github.com/OpenKH/OpenKh
- License: Apache License 2.0 (see `LICENSE` in this folder, copied unmodified from the OpenKH repository)

kh2-rs contains **no OpenKH source code**. It reimplements, in Rust, formats and algorithms that OpenKH documents:
the IDX/IMG archive, its name hashes, LZ decompression, the BAR container and gameplay table layouts.
Each place that does so names OpenKH as its source. See `../../CREDITS.md` for the full list.
OpenKH's KH2 file-name list is downloaded at runtime (`kh2 fetch-names`) and is never committed.
