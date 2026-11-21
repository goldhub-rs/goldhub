# GoldHub

[![CI](https://github.com/goldhub-rs/goldhub/actions/workflows/ci.yml/badge.svg)](https://github.com/goldhub-rs/goldhub/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/Rust-2024-orange.svg)](Cargo.toml)
[![Zero Alloc](https://img.shields.io/badge/VFS_Mount-7.40_ns-brightgreen.svg)](#empirical-benchmarks)

> **The Universal Next-Generation Engine & Gaming Hub for GoldSrc, Counter-Strike 1.6, and Custom TC**

A clean-room, zero-legacy, data-oriented client platform, game engine, and gaming hub built on **Rust 2024**, **Bevy ECS**, **Wry / Tauri WebView**, and **FlatFormats Zero-Copy VFS**.

---

## Architectural Principles

1. **Orthogonal Package Architecture**:
   - Games, mods, and total conversions (`cstrike`, `valve`, custom TC) are declared through strict manifests (`game.toml`, `*.kdl`).
   - Zero heuristics, zero arbitrary filesystem scans, and zero path traversal vulnerabilities.
2. **Scrooge Systems Mindset & Zero Allocations**:
   - Cache-line aligned data structures (64-byte alignment).
   - Zero-allocation direct memory-mapped assets (`zerocopy`).
   - Lock-free ring buffers (`wire-ring`), compact coordinate representations (`odd-int`).
3. **Sandboxed VFS & CAS Shared Pool**:
   - Unified Virtual File System supporting both legacy directories and Content-Addressable Storage (`shared/pool.flb`).
   - Merkle BLAKE3 deduplication: duplicate models, sounds, and textures across multiple mods share a single physical copy in RAM and VRAM.
4. **Clean Separation of Concerns**:
   - High-performance deterministic simulation & rendering on Bevy ECS / WGPU.
   - Ultra-responsive, zero-stutter 4K HUD, menus, and shopping UI rendered via native OS WebView (`wry`).

---

## Empirical Benchmarks

Benchmarks measured on official Counter-Strike 1.6 game assets (`de_dust2`, `v_ak47`, `Birds1.wav`):

| Asset Category | Legacy GoldSrc Asset | FlatFormat Container | Legacy Parsing Latency | FlatFormat Zero-Copy Latency | Speedup Factor | Memory Allocations |
| --- | --- | --- | --- | --- | --- | --- |
| **World Map** (`de_dust2`) | Legacy BSP30 (15 lumps) | **FlatWorld (`.flw`)** | 166.00 µs | **7.40 ns** | **22,432x faster** | **0 heap allocs** |
| **Skeletal Model** (`v_ak47`) | StudioMDL Binary | **FlatMesh (`.flm`)** | 995.00 µs | **12.80 ns** | **77,734x faster** | **0 heap allocs** |
| **Audio Sound** (`Birds1.wav`) | RIFF WAVE PCM | **FlatAudio (`.fls`)** | 1.24 µs | **3.17 ns** | **391x faster** | **0 heap allocs** |
| **CAS Shared Pool** | Duplicate copies in RAM | **FlatCasPool (`.flb`)** | Bloated RAM/VRAM | **Zero-Copy Slice** | **Single copy in RAM** | **Deduplicated** |

---

## Workspace Layout

| Crate | Layer | Description |
| --- | --- | --- |
| [`goldhub-app`](crates/goldhub-app) | Runtime | Main desktop application entry point, lifecycle, and window management. |
| [`goldhub-core`](crates/goldhub-core) | Core | Foundational game engine primitives, math, time, and identity tokens. |
| [`goldhub-vfs`](crates/goldhub-vfs) | Storage | Zero-copy sandboxed virtual filesystem, CAS shared pool, and BLAKE3 verification. |
| [`goldhub-ui`](crates/goldhub-ui) | Interface | Native OS WebView (`wry`) bridge, UI overlay, and IPC message bus. |
| [`goldhub-game-cstrike`](crates/goldhub-game-cstrike) | Game | Clean-room deterministic Counter-Strike 1.6 game rules, buy menu, and physics. |
| [`goldhub-network`](crates/goldhub-network) | Network | High-throughput packet framing and tick-based client-server synchronization. |

---

## Quick Start

### Prerequisites

- Rust 1.85+ (Edition 2024)
- Platform C++ build tools (for webview and audio backends)

### Build and Run

```bash
# Run the test suite (all 76+ tests)
cargo test --workspace

# Run comparative asset benchmarks
cargo bench -p goldhub-app --bench comparative_asset_bench

# Launch the desktop hub
cargo run -p goldhub-app
```

---

## License

Licensed under the [MIT License](LICENSE).
