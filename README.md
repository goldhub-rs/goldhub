# GoldHub

> **The Universal Next-Generation Engine & Gaming Hub**

A clean-slate, zero-legacy, data-oriented client platform and gaming hub built on Rust 2024, Bevy ECS, Wry WebView, and WGPU.

## Features

- **Orthogonal Package Architecture**: Games and modifications (`cstrike`, `valve`, custom TC) are declared via strict `game.toml` and `*.kdl` tree manifests. Zero heuristics, zero path scanning.
- **Scrooge Systems Mindset**: Cache-line aligned data structures (`stitch-data-layout`), lock-free zero-allocation ring buffers (`wire-ring`), compact coordinate representations (`odd-int`).
- **Sandboxed VFS**: Strict directory and memory-mapped file isolation, BLAKE3 integrity verification, and mathematical protection against path traversal.
- **Separation of Concerns**: High-performance 3D pipeline on WGPU/Bevy; zero-stutter 4K HUD and UI overlay on native OS WebView (`wry`).
