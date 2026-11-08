# Master Reference & Bugfix Blueprint: The Modern GoldSrc Architecture

> **Status:** Active Architectural Blueprint  
> **Target Projects:** `GoldSrc.rs` & `goldhub` (Bevy Engine Rust)  
> **Core Principle:** Mechanical Sympathy, Zero Historical Bugs, Sub-Tick Precision & 144+ Hz Fluidity.

---

## 1. Executive Summary & Philosophy

In 1998, GoldSrc shipped with a combination of Quake engine architecture, customized C game DLLs, and 8-bit audio/rendering routines. Over the subsequent 25+ years, millions of players and competitive tournaments discovered thousands of engine limitations, exploits, and quirks:
- Floating-point inaccuracy on ladders and steep ramps causing abrupt drops.
- Jittery skeletal animation updates locked to 20–30 FPS tick frames on 144–360 Hz displays.
- 4:3 FOV stretching on 16:9 / 21:9 ultrawide monitors that distorted viewmodels and clipped arm meshes.
- 8-bit 11025 Hz mono WAV popping and crackling caused by unstripped `LIST INFO` metadata chunks.
- Sound material false positives and memory buffer overflows (`cl_cmdrate` and dynamic light leaks).

Rather than naively cloning 1998 C-code flaws only to fix them one by one, **GoldSrc.rs / goldhub** synthesizes the best-in-class community engineering into a single clean Rust/Bevy codebase.

---

## 2. Reference Arsenal Overview

Our local repository now includes the complete canon of GoldSrc reverse engineering:

| Project | Local Path | Primary Architectural Domain | Key Lessons & Breakthroughs |
| :--- | :--- | :--- | :--- |
| **ReGameDLL_CS** | `references/regamedll/` | Physics, Game Rules, Host Logic | Cleaned C++ prediction parity, ducking speed exploit fixes, bomb planting glitches, hitbox precision. |
| **ReHLDS** | `references/rehlds/` | Engine, NetChannel, Memory | Uncapped net rates, delta compression safety, buffer overflow protections, dynamic allocation. |
| **NextClient** | `references/nextclient/` | Client UX, Modern VGUI, Audio | High-speed multi-threaded downloads, Steam avatar CDN, modern VGUI/HUD, Discord RPC, raw input. |
| **CS_Patches (Garey27)** | `references/cs_patches/` | Binary Engine Patches | Hor+ widescreen FOV math, duck hull bound parity (`player_maxs[1][2] = 32.0`), 48kHz voice, symbols view. |
| **cs16-goldsrc-client** | `references/cs16_goldsrc_client/` | Client.dll ABI & Prediction | Clean open-source `client.dll`, client-side prediction `HUD_PostRunCmd`, overview radar, spectator HUD. |
| **Xash3D FWGS** | `references/xash3d-fwgs/` | Full Engine Architecture | Decoupled cross-platform physics, modern VFS, robust WAV audio mixing, BSP30/31 support. |
| **GoldClient** | `goldclient.ru` changelog | Performance & Interpolation | Backport of HL25 `cl_fixmodelinterpolationartifacts`, viewmodel sway/lag scaling, progressive demo loading. |

---

## 3. The 5-Layer Bugfix & Modernization Matrix

```
       +-----------------------------------------------------------+
       | Layer 5: Renderer, Shaders & UX (NextClient / TonyMcMap) |
       +-----------------------------------------------------------+
       | Layer 4: Network, Prediction & Lag Comp (ReHLDS / Subtick)|
       +-----------------------------------------------------------+
       | Layer 3: Audio & DSP Engine (16-bit 44.1kHz, Spatial HRTF)|
       +-----------------------------------------------------------+
       | Layer 2: StudioModel (.mdl) & Viewmodel Pipeline (HL25)   |
       +-----------------------------------------------------------+
       | Layer 1: Physics & World Collision Core (ReGameDLL / SMA) |
       +-----------------------------------------------------------+
```

### Layer 1: Physics, World Collision & Player Movement

| Historical GoldSrc Bug / Flaw | Reference Solution | Rust/Bevy Architecture (`goldhub-physics`) |
| :--- | :--- | :--- |
| **Ladder top sticking / dismount lock:** Player remains stuck on ladder when climbing onto platforms because expanded hull (`+36` units) intersects ladder AABB. | `regamedll: PM_LadderMove` (`onFloor && normal > 0` dismount logic) | Transition player to `on_ground = true, on_ladder = false` when feet contact floor with forward wish velocity. |
| **A/D ladder throw-down:** Strafe velocity converts into negative vertical velocity when viewing angle has vertical pitch. | `regamedll: PM_LadderMove` (decoupling lateral from vertical) | Strictly project lateral strafe `[-perp * wish_vel]` onto ladder wall plane; never invert ladder normal. |
| **Duck-jump view height jitter:** Eye height snaps abruptly between 12 and 28 units during air crouches. | `regamedll: PM_Duck` (smooth viewheight interpolation `duck_time_ms`) | Continuous sub-tick exponential smoothing for eye offset between standing (`+28`) and crouching (`+12`). |
| **Bunnyhop speed explosion on tickrate changes:** Higher tickrates (100 / 128) increase acceleration multiplier exponentially. | `regamedll: sv_airaccelerate` normalization | Constant physical acceleration delta invariant to framerate or tickrate (`dt * accel * wishspeed`). |
| **Edge sticking & ramp sliding loss:** Player loses horizontal momentum when grazing sharp BSP poly edges. | `ReHLDS: PM_ClipVelocity` | Multi-plane projection clipping (`numplanes > 1`) with epsilon protection against stuck loops. |

---

### Layer 2: StudioModel (`.mdl`), Viewmodel & Skeletal Animation

| Historical GoldSrc Bug / Flaw | Reference Solution | Rust/Bevy Architecture (`goldhub-app` / `goldhub-formats`) |
| :--- | :--- | :--- |
| **Jerky 20–30 FPS animations at 144+ Hz:** Viewmodels animate at low keyframe rates, appearing stuttery despite high framerates. | `GoldClient: cl_fixmodelinterpolationartifacts` & HL25 | Linear SLERP quaternion interpolation between animation keyframe samples at exact display delta-time. |
| **Widescreen FOV stretch & arm clipping:** Non-4:3 aspect ratios (16:9, 21:9) stretch hands or reveal empty unmodeled weapon meshes. | `CS_Patches: widescreen.cpp` & `NextClient` | Compute Hor+ FOV keeping vertical angle invariant: `new_y = 2 * atan(tan(fov * pi / 360) * 0.75)`, scale X. |
| **Moving platform animation tearing:** Models riding on `func_door` or `func_train` jitter relative to platform. | `HL25 / GoldClient 2.4.0` | Frame-relative origin transformation before skeletal bone matrix decomposition. |
| **Muzzle flash & shell eject desync:** Weapon attachment points (`R_StudioGetAttachment`) lag 1 frame behind camera. | `FuryBaM: HUD_PostRunCmd` | Synchronous forward pass: compute attachment transforms immediately after viewmodel matrix update. |

---

### Layer 3: Audio & DSP Subsystem

| Historical GoldSrc Bug / Flaw | Reference Solution | Rust/Bevy Architecture (`goldhub-audio`) |
| :--- | :--- | :--- |
| **WAV crackle & raspiness:** CS 1.6 8-bit 11025 Hz WAV files containing trailing `LIST INFO` metadata chunks cause audible pops. | `goldhub-audio: transcode_to_clean_16bit_44khz` | Upsample 11025 Hz -> 44100 Hz 16-bit PCM with linear interpolation and 32-sample anti-click micro-fades. |
| **Material search false positives (Metal on Concrete):** Short or empty texture queries falsely matched `cstrike_CE2XCng` or `-0A747PLANE_X4`. | `ReGameDLL: PM_FindTextureType` | Binary search with forward prefix matching only (`search.starts_with(entry)`); strict fallback to Concrete. |
| **Sound buffer overflow during heavy gunfire:** Sound channel exhaustion drops footstep or reload sounds. | `Xash3D: S_MixBuffer` | Infinite dynamic pooling via Bevy `AudioSource` with auto-despawn on playback completion. |

---

### Layer 4: Network, Prediction & Lag Compensation

| Historical GoldSrc Bug / Flaw | Reference Solution | Rust/Bevy Architecture (`goldhub-net`) |
| :--- | :--- | :--- |
| **Server/client footstep prediction desync:** Local player prediction plays footstep sound on frame N, but server reconciliation plays it again on frame N+2. | `cs16-goldsrc-client: HUD_PostRunCmd` | Client prediction suppress bitmask: suppress incoming server `CHAN_BODY` footstep packets for local player entity. |
| **Network packet rate bottleneck:** Hardcoded limit of 100/101 cmdrate causes input delay on modern 1000 Hz mice. | `ReHLDS: cl_cmdrate uncap` | Sub-tick input packet serialization with ring-buffered user commands capable of 250+ subtick updates. |
| **Lag compensation extrapolation glitch:** Fast-moving targets extrapolate through solid walls on high ping. | `ReGameDLL: SV_CheckLagCompensation` | Bounded BSP collision traces during backward rewind; never extrapolate across solid plane boundaries. |

---

### Layer 5: Renderer, Shaders & User Experience

| Historical GoldSrc Bug / Flaw | Reference Solution | Rust/Bevy Architecture (`goldhub-app`) |
| :--- | :--- | :--- |
| **Primitive 2D green radar dots without terrain:** Original radar gave no positional orientation or map awareness. | `GoldClient & NextClient: overview radar` | Orthographic top-down map render (`overviews/<map>.tga`), dynamic view frustum cone, teammate elevation arrows, dynamic scaling (`hud_radar_mapscale_dynamic`). |
| **Monochrome low-info killfeed:** Missing headshot, wallbang, smoke, and assist indicators. | `GoldClient: modern killfeed` | High-res SVG/raster weapon glyphs, colored icons for headshot, wallbang penetration, blind assist, and local player highlight border. |
| **Physics framerate coupling (`fps_max > 100` glitches):** Running at 240+ FPS causes bunnyhop slowdowns, jump height loss, and stamina recharge bugs. | `GoldClient: cl_cmdtickrate` | Decouple physics tick loop from render FPS: fixed sub-tick accumulator (e.g. 100/128 Hz) independent of display refresh rate. |
| **Washed-out / overly dark GoldSrc lighting:** Original software/OpenGL lightmaps suffered from color banding and overbright clipping. | `GoldClient: gl_use_shaders` & Bevy PBR | `TonyMcMapface` Tonemapping + exposure scaling (`1200 lux`) converting normalized luxels into linear radiance. |
| **Texture wrapping seam artifacts:** Cutout decals and repeating wall textures clamp incorrectly at edges. | `Xash3D: R_LoadTexture` | Explicit `ImageAddressMode::Repeat` for standard textures and `ClampToEdge` for atlas lightmaps. |
| **Mouse input pointer ballistics:** Windows default pointer ballistics cause inconsistent flick shots. | `NextClient: raw mouse input` | Native Win32 / Bevy raw mouse delta accumulation without desktop acceleration curves. |

---

## 4. Deep Dive: Tactical Overview Radar & Killfeed Systems

### 4.1 Overview Radar Architecture (Top-Down Ortho Map)
As illustrated by GoldClient / modern CS clients, the radar pipeline consists of 4 distinct rendering passes:
1. **Background Layer (`overviews/<mapname>.tga` & `.txt`):**
   - Reads map origin coordinates `(x, y, z)` and zoom factor.
   - Computes player offset: `u = (player.x - origin.x) * zoom`, `v = (origin.y - player.y) * zoom`.
   - Supports circular/rounded clipping mask with dynamic auto-zoom (`hud_radar_mapscale_dynamic`).
2. **Objective Layer:**
   - Bomb sites `A` and `B` rendered at fixed world targets (yellow brackets in HUD).
   - Dropped C4 / Hostages with blinking state and distance clamping at radar edge.
3. **Player & View Cone Layer:**
   - Centered circular blip (local player).
   - Directional FOV viewing arc (frustum cone) projecting yaw orientation into 2D space.
4. **Teammates & Elevation:**
   - Teammate blips with directional pointer.
   - Elevation status: `^` icon if teammate `Z > player.Z + 64`, `v` icon if `Z < player.Z - 64`.

### 4.2 Modern Killfeed Pipeline
- Weapon death notice icons rendered in right-hand stack with `hud_deathnotice_time` decay.
- Modular badges: `[Attacker]` + `[Penetration Icon]` + `[Headshot Icon]` + `[Weapon Icon]` + `[Victim]`.
- Local player highlight: golden/red glowing accent border when involved in the frag.

---

## 5. Immediate Execution Roadmap

Following the user-approved strategy:
1. **Vector 2 (In Progress):** ViewModel weapon rendering (`v_knife.mdl`) with skeletal bone matrix evaluation, Hor+ FOV math, and smooth animation interpolation.
2. **Vector 1 (Completed):** 16-bit 44.1 kHz clean PCM audio engine, `materials.txt` strict prefix matching, and walkable ground surface tracing.
3. **Vector 3 (Next):** Ladder dismount & A/D strafe stabilization in `goldhub-physics` applying the decoupled normal projection.
