# Master Reference & Bugfix Blueprint: The Modern GoldSrc Architecture

> **Status:** Active Architectural Master Blueprint  
> **Target Projects:** `GoldSrc.rs` & `goldhub` (Bevy Engine Rust)  
> **Core Principle:** Mechanical Sympathy, Zero Historical Bugs, Sub-Tick Precision, Multi-Vector Parity & 144+ Hz Fluidity.

---

## 1. Executive Summary & Philosophy

In 1998, GoldSrc shipped as an evolution of the Quake engine, customized C game DLLs, and 8-bit audio/rendering routines. Over the subsequent 25+ years, millions of players and competitive tournaments uncovered thousands of engine limitations, exploits, and quirks:

- Floating-point inaccuracy on ladders and steep ramps causing abrupt drops.
- Jittery skeletal animation updates locked to 20–30 FPS tick frames on 144–360 Hz displays.
- 4:3 FOV stretching on 16:9 / 21:9 ultrawide monitors distorting viewmodels and clipping meshes.
- 8-bit 11025 Hz mono WAV popping and crackling caused by unstripped `LIST INFO` metadata chunks.
- Command buffer overflows (`cl_cmdrate`) and high-FPS physics coupling causing player stutter.
- Server-side slowhacking (malicious cvar overwrites, key rebinds, client file modification).
- Primitive HUD elements (blind 2D radar dots, monochrome killfeed, distorted sniper scopes).

Rather than naively cloning 1998 C-code bugs only to patch them retroactively, **GoldSrc.rs / goldhub** synthesizes the best community reverse engineering into a single unified Rust/Bevy codebase.

---

## 2. Reference Arsenal Overview

Our local workspace (`D:/Repo/GoldSrc.rs/references/`) and community canon include:

| Project | Domain | Primary Lessons & Breakthroughs |
| :--- | :--- | :--- |
| **ReGameDLL_CS** | Physics & Game Rules | Cleaned C++ prediction parity, ducking speed exploit fixes, bomb planting glitches, hitbox precision. |
| **ReHLDS** | Engine & NetChannel | Uncapped net rates, delta compression safety, buffer overflow protections, dynamic allocation. |
| **NextClient** | Client UX, Security & Shaders | Command origin security, Cvars sandbox, multi-threaded FastDL, dynamic BSP vector radar, raw input, PBR studio shaders. |
| **GoldClient** | Modern Engine QoL & ConVars | HL25 `cl_fixmodelinterpolationartifacts`, `cl_cmdtickrate` (physics decoupling), dynamic radar scaling, extended killfeed, 100% portable configs. |
| **CS_Patches (Garey27)** | Binary Engine Fixes | Hor+ widescreen FOV math, duck hull bound parity (`player_maxs[1][2] = 32.0`), 48kHz voice with AGC, UTF-8 console fix. |
| **cs16-goldsrc-client** | Client.dll ABI & Prediction | 44 PE exports, `HUD_PostRunCmd` prediction loop, punchangle decay, undistorted circular sniper scope mask, modern spectator cards. |
| **Xash3D FWGS** | Full Engine Architecture | Decoupled cross-platform physics, modern VFS, dual-matrix studio skinning, gait bone splitting, 200ms sequence crossfade, audio wrap guard `0x40000000`. |

---

## 3. The 6-Layer Architecture Matrix

```text
       +-----------------------------------------------------------+
       | Layer 6: Security, Sandbox & FastDL (NextClient / ReHLDS) |
       +-----------------------------------------------------------+
       | Layer 5: Renderer, Shaders & Modern UX (GoldClient/NextCl)|
       +-----------------------------------------------------------+
       | Layer 4: Network, Prediction & Lag Comp (ReGameDLL/Client)|
       +-----------------------------------------------------------+
       | Layer 3: Audio & DSP Engine (16-bit 44.1kHz, Spatial HRTF)|
       +-----------------------------------------------------------+
       | Layer 2: StudioModel (.mdl) & Viewmodel Pipeline (HL25)   |
       +-----------------------------------------------------------+
       | Layer 1: Physics & World Collision Core (ReGameDLL / SMA) |
       +-----------------------------------------------------------+
```

---

### Layer 1: Physics, World Collision & Player Movement

| Historical GoldSrc Bug / Flaw | Community Reference Solution | Rust/Bevy Architecture (`goldhub-physics`) |
| :--- | :--- | :--- |
| **Ladder top sticking / dismount lock:** Player remains stuck on ladder when climbing onto platforms because expanded hull (`+36` units) intersects ladder AABB. | `ReGameDLL: PM_LadderMove` (`onFloor && normal_comp > 0` dismount logic) | Transition player to `on_ground = true, on_ladder = false` when feet contact floor with forward wish velocity. |
| **A/D ladder throw-down:** Strafe velocity converts into negative vertical velocity when viewing angle has vertical pitch. | `ReGameDLL: PM_LadderMove` (decoupling lateral from vertical) | Strictly project lateral strafe `[-perp * wish_vel]` onto ladder wall plane; never invert ladder normal. |
| **Duck Hull Bounds Desync:** Client `CL_GetPlayerHulls` left duck maxs[2] at 18.0 (height 36), while server simulated 32.0 (height 50), desyncing air vent traversal. | `CS_Patches: player_maxs[1][2] = 32.0` | Exact hull parity: Standing `Aabb(-16..16, -16..16, -36..36)`, Ducking `Aabb(-16..16, -16..16, -18..32)`. |
| **Duck-jump view height jitter:** Eye height snaps abruptly between 12 and 28 units during air crouches. | `ReGameDLL: PM_Duck` (smooth viewheight interpolation `duck_time_ms`) | Continuous sub-tick exponential smoothing for eye offset between standing (`+28`) and crouching (`+12`). |
| **Bunnyhop speed explosion on tickrate changes:** Higher tickrates (100 / 128) increase acceleration multiplier exponentially. | `ReGameDLL: sv_airaccelerate` normalization | Constant physical acceleration delta invariant to framerate or tickrate (`dt * accel * wishspeed`). |
| **High FPS Movement Stutter (`fps_max > 100`):** Physics tied to render frames causes players to get stuck on slopes and platforms. | `GoldClient: cl_cmdtickrate` | Completely decouple physics tick loop from render FPS: fixed sub-tick accumulator (100/128 Hz) independent of display Hz. |
| **Edge sticking & ramp sliding loss:** Player loses horizontal momentum when grazing sharp BSP poly edges. | `ReHLDS: PM_ClipVelocity` | Multi-plane projection clipping (`numplanes > 1`) with epsilon protection against stuck loops. |

---

### Layer 2: StudioModel (`.mdl`), Viewmodel & Skeletal Animation

| Historical GoldSrc Bug / Flaw | Community Reference Solution | Rust/Bevy Architecture (`goldhub-app` / `goldhub-formats`) |
| :--- | :--- | :--- |
| **Jerky 20–30 FPS animations at 144+ Hz:** Viewmodels animate at low keyframe rates, appearing stuttery despite high framerates. | `GoldClient: cl_fixmodelinterpolationartifacts` & HL25 | Linear SLERP quaternion interpolation between animation keyframe samples at exact display delta-time. |
| **Moving platform animation tearing:** Models riding on `func_door` or `func_train` jitter relative to platform. | `HL25 / GoldClient 2.4.0` | Frame-relative origin transformation before skeletal bone matrix decomposition. |
| **Upper/Lower body gait animation glitch:** Player third-person models jitter when running and shooting simultaneously. | `Xash3D: gl_studio.c` (gait bone splitting) | Split bone hierarchy at `"Bip01 Spine"`: pelvis/legs follow gaitsequence, spine/head/arms follow weapon sequence. |
| **Attachment & dynamic light jitter:** Studio weapon muzzle attachments jitter 1 frame behind camera. | `Xash3D: gl_studio.c` (`bonestransform` vs `lighttransform`) | Dual-matrix pipeline: decoupling bone deformation matrix from rigid attachment/light transform matrix. |
| **Abrupt animation sequence switches:** Model snaps abruptly when transitioning between idle, fire, and reload. | `Xash3D & cs16-client`: 200ms sequence crossfade | Quaternion spherical linear interpolation (`slerp`) blending prev and current sequence poses over 200ms. |
| **Camera shake during reload/fire:** Weapons lack visceral animated camera kick. | `NextClient: camera.cpp` (`"camera"` bone extraction) | Query studio header for `"camera"` bone; inject bone rotation angles directly into camera orientation. |
| **Viewmodel vertical pitch shift:** Viewmodel moves awkwardly up and down when looking at sky/ground. | `NextClient: viewmodel_disable_shift 1` | Lock viewmodel base Z coordinate in camera space, preserving natural perspective sightlines. |
| **Viewmodel lag and sway:** Weapons feel stiff and robotic when turning. | `NextClient: viewmodel_lag_style` (HL2 delta front / CS:S ring buffer) | Smooth lagging quaternion orientation buffer trailing camera angular velocity. |

---

### Layer 3: Audio & DSP Subsystem

| Historical GoldSrc Bug / Flaw | Community Reference Solution | Rust/Bevy Architecture (`goldhub-audio`) |
| :--- | :--- | :--- |
| **WAV crackle & raspiness:** CS 1.6 8-bit 11025 Hz WAV files containing trailing `LIST INFO` metadata chunks cause audible pops. | `goldhub-audio: transcode_to_clean_16bit_44khz` | Upsample 11025 Hz -> 44100 Hz 16-bit PCM with linear interpolation and 32-sample anti-click micro-fades. |
| **Material search false positives (Metal on Concrete):** Short or empty texture queries falsely matched `cstrike_CE2XCng` or `-0A747PLANE_X4`. | `ReGameDLL: PM_FindTextureType` | Binary search with forward prefix matching only (`search.starts_with(entry)`); strict fallback to Concrete. |
| **Sound cache overflow crashes:** Servers with many custom sounds exceed `MAX_SFX` (512/1024), crashing client. | `NextClient: S_UnloadSounds` (`Cache_Free`) | Unbounded dynamic audio asset pooling via Bevy `AudioSource` with auto-despawn on playback completion. |
| **Mix buffer integer overflow:** Long game sessions cause sample position to wrap around, creating deafening screeching. | `Xash3D: S_MixBuffer` wrap guard (`0x40000000`) | Modulo sample counter resets; 64-bit sample indexing completely immune to 32-bit wrapping. |
| **Low quality voice chat:** Legacy Miles/Speex codec at 8000 Hz sounds muffled and robotic. | `CS_Patches: Steam Voice 48kHz + AGC` | Opus codec at 48000 Hz with automatic gain control (AGC) and noise gate filtering. |

---

### Layer 4: Network, Prediction & Lag Compensation

| Historical GoldSrc Bug / Flaw | Community Reference Solution | Rust/Bevy Architecture (`goldhub-net`) |
| :--- | :--- | :--- |
| **Server/client footstep prediction desync:** Local player prediction plays footstep sound on frame N, but server reconciliation plays it again on frame N+2. | `cs16-goldsrc-client: HUD_PostRunCmd` | Prediction bitmask: suppress incoming server `CHAN_BODY` footstep packets for local player entity. |
| **Punchangle & recoil smoothing:** Weapon primary attacks snap screen instantly, causing disorientation. | `cs16-goldsrc-client: PM_DropPunchAngle` & smoothed crosshair offset | Sub-tick punchangle exponential decay (`punchangle = punchangle * (1.0 - dt * 10.0)`). |
| **Weapon timer clamping:** Reload and switch timers float unpredictably when client FPS fluctuates. | `cs16-goldsrc-client: ItemPostFrame` timer bounds | Clamped weapon timers (-1.0f primary attack, -0.001f secondary/idle/reload) preventing stuck states. |
| **Network packet rate bottleneck:** Hardcoded limit of 100/101 cmdrate causes input delay on modern 1000 Hz mice. | `ReHLDS: cl_cmdrate uncap` | Sub-tick input packet serialization with ring-buffered user commands capable of 250+ subtick updates. |
| **Lag compensation extrapolation glitch:** Fast-moving targets extrapolate through solid walls on high ping. | `ReGameDLL: SV_CheckLagCompensation` | Bounded BSP collision traces during backward rewind; never extrapolate across solid plane boundaries. |

---

### Layer 5: Renderer, Shaders & User Experience

| Historical GoldSrc Bug / Flaw | Community Reference Solution | Rust/Bevy Architecture (`goldhub-app`) |
| :--- | :--- | :--- |
| **4:3 FOV Hor- distortion on modern displays:** 16:9/21:9 monitors crop top and bottom, reducing field of view. | `CS_Patches & NextClient: Hor+ Widescreen FOV` | Exact Hor+ math: $FOV_{hor+} = 2 \cdot \arctan\left(\tan\left(\frac{FOV \cdot \pi}{360}\right) \cdot (aspect \cdot 0.75)\right) \cdot \frac{180}{\pi}$. |
| **Distorted oval sniper scope on widescreen:** Valve's TriAPI triangle fan scope stretched into an ellipse on 16:9. | `cs16-client & NextClient: circular scope mask` | Fullscreen WGSL shader calculating normalized aspect-corrected distance from screen center: $r = \min(w, h) / 2$. |
| **Blind 2D radar dots without terrain context:** Original radar gave zero situational awareness of map layout or height. | `GoldClient & NextClient: Overview Tactical Radar` | Dynamic vector/texture overview radar with FOV cone, height arrows, bomb site brackets, dynamic scaling (`hud_radar_mapscale_dynamic`). |
| **Monochrome low-info killfeed:** Missing headshot, wallbang, smoke, and assist indicators. | `GoldClient: Modern Killfeed` | High-res SVG weapon glyphs, colored icons for headshot, wallbang penetration, blind assist, and local player highlight border. |
| **Windows mouse pointer ballistics:** Win32 `WM_MOUSEMOVE` subject to Windows OS acceleration curves. | `NextClient: m_rawinput 1` | Direct Winit / RawInput `DeviceEvent::MouseMotion` hardware counts bypassing desktop ballistics. |
| **Console unicode corruption:** Cyrillic / UTF-8 characters >= 0x80 corrupted by signed `jle` char comparison. | `CS_Patches: unsigned char conversion` | Native UTF-8 Rust `String` / `char` representation throughout console and chat. |

---

### Layer 6: Security, Sandbox & Asset Streaming

| Historical GoldSrc Vulnerability | Community Reference Solution | Rust/Bevy Architecture (`goldhub-core`) |
| :--- | :--- | :--- |
| **Slowhack Malicious Server Commands:** Rogue servers bind keys, overwrite `config.cfg`, or issue `quit`/`connect` redirects via `svc_stufftext`. | `NextClient: protector.cpp & CmdChecker` | Type-safe `CommandOrigin` capability token. Console accepts user commands; server stufftext rejected for protected commands (`bind`, `exec`, `writecfg`, `connect`). |
| **Slowhack Cvar Overwrites:** Servers permanently ruin client settings (`sensitivity`, `cl_forwardspeed`, `fps_max`). | `NextClient: cl_cvar_sandbox.cpp` | Dual-storage `CvarRegistry`: `local_values` vs `server_overrides`. Server changes apply to sandbox memory only; disconnect clears overrides with zero disk I/O. |
| **Malicious Server Redirects:** Rogue servers inject UDP challenge packets redirecting players to scam servers. | `NextClient: protector.cpp:71` (drop `'L'` challenge packets) | Inspect out-of-band connectionless packets; reject server redirect challenges without user interaction. |
| **Blocking, Single-Threaded FastDL:** Downloading map assets freezes the client and stalls frame pump. | `NextClient: HttpDownloadManager (CPR/curl pool)` | Asynchronous Tokio + `reqwest` connection pool streaming assets directly into Bevy's `AssetServer` via `AsyncComputeTaskPool`. |
| **Malicious Asset Overwrites:** Rogue servers download custom sounds/models overwriting original client files. | `NextClient: cl_private_resources.cpp` | Virtual File System (`Vfs`) mounting server content to sandboxed namespaces (`server://<sanitized_endpoint>/`). |
| **Registry Dependency & Pollution:** Settings stored in Windows Registry (`HKCU\Software\Valve`). | `GoldClient 2.5.7.0: portable file storage` | 100% portable file-based TOML configurations (`config.toml`, `.local.toml`). |

---

## 4. Deep Dive: Tactical Overview Radar System

As demonstrated in GoldClient and NextClient, the modern tactical radar is a multi-tier HUD subsystem:

```text
+--------------------------------------------------------------+
| 1. Dynamic Map Overview Layer                                |
|    - Vector geometry extracted from BSP floor leaves         |
|    - Or orthographic top-down map render (`overviews/*.tga`) |
|    - Dynamic Auto-Scale (`hud_radar_mapscale_dynamic 1`)     |
|    - Circular shader mask with soft fading edge              |
+--------------------------------------------------------------+
| 2. Objectives & World Points Layer                           |
|    - Bomb sites [A] and [B] (yellow brackets)                |
|    - Dropped C4 / Hostages (pulsing red icon, edge clamped)   |
+--------------------------------------------------------------+
| 3. Local Player & Frustum Cone Layer                         |
|    - Local player blip (cyan glowing circle)                 |
|    - Directional FOV frustum cone projected into 2D space    |
+--------------------------------------------------------------+
| 4. Teammates, Enemies & Elevation Layer                      |
|    - Teammates: CT Blue / T Orange                           |
|    - Health rings around blips (`hud_radar_showhealth 1`)    |
|    - Elevation indicators: Up arrow (`^`) / Down arrow (`v`) |
|    - Wallhack leak guard: enemies obscured by walls hidden   |
+--------------------------------------------------------------+
| 5. Contextual Zone Label                                     |
|    - Location name displayed beneath radar (e.g. "B Site")   |
+--------------------------------------------------------------+
```

### Full Radar ConVar Suite (GoldClient / NextClient Parity)

```ini
hud_radar                         1      // 0 = off, 1 = on, 2 = semi-transparent
hud_radar_mapscale_dynamic        1      // Auto-scale map to fit all visible entities
hud_radar_mapscale_dynamic_min    0.35   // Minimum zoom clamp (0.01 to 3.5)
hud_radar_scale                   1.0    // Radar UI size on screen
hud_radar_alpha                   200    // Background opacity (0-255)
hud_radar_locked                  0      // 0 = rotates with player, 1 = north locked up
hud_radar_square                  0      // 0 = modern circular mask, 1 = classic square
hud_radar_square_with_scoreboard  1      // Expand to square when viewing Tab scoreboard
hud_radar_showhealth              1      // Show circular health bar around teammate dots
hud_radar_always_centered         1      // 1 = centered on local player, 0 = map overview
hud_radar_icon_scale_min          0.8    // Minimum blip icon size
hud_radar_bg                      1      // Framebuffer blur background
```

---

## 5. Deep Dive: Modern Killfeed (DeathNotice)

The killfeed is decoupled from standard console prints into a dedicated GPU-rendered notification stack:

- **Decay Timer:** Governed by `hud_deathnotice_time` (default 6.0s) with smooth alpha fade-out over the final 500ms.
- **Local Player Accent:** When local player is attacker or victim, render high-contrast tinted background plate (golden/red glow).
- **Badge Indicators:**
  - `[Headshot]`: Red skull glyph for headshot kills.
  - `[Wallbang]`: Penetration bullet-through-wall glyph.
  - `[Smoke]`: Kill through volumetric smoke cloud.
  - `[Blind]`: Kill while fully flash-banged.
  - `[Revenge / Domination]`: Nemesis / Revenge icons.
- **Weapon Glyph Registry:** Full SVG/raster icon set supporting all 30 CS weapons, knives, C4 explosion, worldspawn fall, plus custom community entities (`tracktrain`, `lasermine`, `sentrygun`, `molotov`).

---

## 6. Implementation Architecture in Bevy ECS

```rust
// Physics & Movement decoupled from Render FPS
app.add_systems(
    FixedUpdate,
    (
        assemble_subtick_usercmds,
        pm_ladder_move,
        pm_air_accelerate,
        pm_duck_smooth,
        pm_drop_punchangle,
        reconcile_server_snapshots,
    ).chain()
);

// High-Hz View & Visuals (144+ FPS)
app.add_systems(
    Update,
    (
        update_widescreen_horplus_fov,
        evaluate_skeletal_bone_slerp,
        apply_viewmodel_dynamics,
        update_tactical_radar_mesh,
        render_modern_killfeed,
    )
);
```

---

## 7. Immediate Execution Vector

1. **Vector 2 (Active):** ViewModel weapon rendering (`v_knife.mdl`) with skeletal bone matrix evaluation, Hor+ FOV math, and smooth animation interpolation in `goldhub-app`.
2. **Vector 1 (Completed):** 16-bit 44.1 kHz clean PCM audio engine, `materials.txt` strict prefix matching, and walkable ground surface tracing.
3. **Vector 3 (Next):** Ladder dismount (`onFloor && normal_comp > 0.0`) and A/D strafe stabilization in `goldhub-physics`.
