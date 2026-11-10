# Audit Dialogue

User:

Привет, мне нужен полнцоенный аудит того, на каком уровне происходит перенос кс 1.6 на rust. Что перенесено корректно, что с ошибками. Чего нам не хвататет и остоется перенести. Цель проекта создать переписать движок GoldSrc на rust на основе ReGameDLL_CS и ReHLDS с учетом bugfixed версий клиентов игры от камьюнити по типу NextClient, GoldClient, GSClient, CS_Patches, Xash3D, а "D:Repo\GoldSrc.rs"  послужит SDK для моддинга как на старых площадках так и для нового. CS1.6 была выбрана, т.к. я играю в нее 13 лет и знаю множество механик и смогу тестировать и валидировать свою же работу. Буду восстанавливать движок на примере готовой игры. Также помимо самого движка будет реализован клиент-хаб по примеру того, что изначально хотели получить valve. Мне нужна не поверхностнаяв работа, а что бы ты изучил все референсы из "D:\Repo\GoldSrc.rs\references" максимально детально, чтобы можно было восстановить код ориг движка и портировать его на rust, попутно исправляя баги, НО важно сохранить тот самый вайб игры - не менять физику, не менять механики, которые многие считают багами, но по сути являются особенностями математики движка. Улучшения будут вносится позже + сама архитектура из старого монолита будет после переноса переписываться на stitch-rs (SMA) + все остальные специально подготовленные проекты, которые я писал по ходу развития GoldSrc.rs по типу modtree-rs, machine-rs, statefs-rs, abi-barrier-rs, narrow-ptr-rs, odd-int-rs, proc-safety-rs, wire-ring-rs

---

Agent:

## Objective

- Провести полный (не поверхностный) аудит переноса движка CS 1.6 GoldSrc на Rust: что перенесено корректно, что с ошибками, чего не хватает.
- Цель проекта: переписать GoldSrc на Rust по ReGameDLL_CS + ReHLDS, учесть bugfix-клиенты (NextClient, GoldClient, GSClient, CS_Patches, Xash3D); `D:\Repo\GoldSrc.rs` — SDK для моддинга, `D:\Repo\goldhub` — клиент-хаб (изначальная идея Valve). Сохранить оригинальную физику/механики, улучшения — позже; затем миграция на stitch-rs (SMA) + modtree-rs, machine-rs, statefs-rs, abi-barrier-rs, narrow-ptr-rs, odd-int-rs, proc-safety-rs, wire-ring-rs.

## Important Details

- Два репозитория: `D:\Repo\goldhub` — реальный движок/клиент на Bevy (~22k LOC, git remote `https://github.com/goldhub-rs/goldhub.git`); `D:\Repo\GoldSrc.rs` — SDK/runtime (корень не git; подрепы `goldsrc-runtime`, `goldsrc-cli`, `goldsrc-sdk`, `goldsrc-sdk-cstrike`, `goldsrc-plugins-standard`).
- goldhub в MCP-графе НЕ проиндексирован (индексированы `GoldSrc.rs`, `D-Repo-goldsrc-rs`, `D-Repo-stitch-rs`, `D-Repo-grlg-geo`; `D:/Repo/goldsrc-rs` как папки не существует) — читать файлами/grep.
- Workspace goldhub: `crates/goldhub-manifest`, `goldhub-vfs`, `goldhub-formats`, `goldhub-net`, `goldhub-physics`, `goldhub-audio`, `goldhub-app`.
- Блюпринт: `D:/Repo/goldhub/docs/MASTER_REFERENCE_AND_BUGFIX_BLUEPRINT.md` ≈ `D:/Repo/GoldSrc.rs/docs/...` (мелкие отличия).
- Референсы: `amxmodx, cs16_goldsrc_client, cs_patches, hlsdk, metamod-r, nextclient, parsers, reapi, regamedll, rehlds, xash3d-fwgs`. GoldClient/GSClient в references ОТСУТСТВУЮТ.
- В goldhub нет `todo!/unimplemented!/FIXME/TODO` — незавершённость скрыта «рабочими» заглушками.
- git status goldhub: изменены `combat/recoil.rs, combat/weapon.rs, entity/mod.rs, hud/mod.rs, lib.rs, player/mod.rs, studio/{animation,viewmodel,world_entity}.rs, formats/codecs/legacy/{mod,wad}.rs, tests/real_assets_test.rs, physics/movement.rs, docs blueprint`; новые `crates/goldhub-app/src/sprite/`, `crates/goldhub-formats/src/codecs/legacy/spr.rs`.
- Физика: фиксированный 128 Гц (`SUBTICK_HZ=128`, `FIXED_DT=1/128`) — отклонение от оригинального переменного шага (`cmd.msec`, ~100 tick), противоречит «не менять физику».
- Duck hull неверен: Rust `PLAYER_HULL_MIN/MAX_DUCKING=[-16,-16,-18]/[16,16,18]`; канон `VEC_DUCK_HULL_MIN/MAX=(-16,-16,-18)/(16,16,32)` (высота 50).
- Отсутствует вода целиком, `basevelocity` (поезда/двери/платформы), `PM_CheckVelocity` (NaN/clamp), edgefriction в `PM_Friction`, spectator/noclip, fall damage/landing sound/punchangle, punchangle+roll в углах, `PM_AddToTouched`.
- `PM_Accelerate`: Rust `wishspeed.max(stopspeed)` и без `pmove->friction`; канон `accel*frametime*wishspeed*pmove->friction`.
- Прыжок: Rust вычитает `gravity*0.5*dt` при прыжке (лишнее); канон `velocity[2]=sqrt(2*800*45)=268.328`, fuser2 penalty, `fuser2=1315.789429`, запрет прыжка в полном приседе; bunny-cap всегда, канон — по `sv_enablebunnyhopping`.
- Плавный unduck 200 мс (`TIME_TO_UNDUCK`) — добавление; ReGameDLL `PM_UnDuck` мгновенно ставит `view_ofs=17`.
- `PM_WalkMove` без oldonground-guard, basevelocity, waterjump; нет `IN_RUN`.
- Ladder: канон только при `!dead && !(FL_ONTRAIN)`; Rust всегда; мелкие отличия onFloor и аналогового fallback forward/right.
- `weapon.rs`: только 7 `WeaponId` (Knife=29, Glock18=17, USP45=16, Deagle=26, AK47=28, M4A1=22, AWP=18) против 30 классов/31 ID в ReGameDLL (`weapontype.h:33-62`).
- Нет реализаций: Hor+ FOV, raw input, radar, slowhack/sandbox, FastDL, HRTF, Opus, killfeed, cvar/config, stufftext-защиты.
- AGENTS.md требует «Lead Architect» режим: сначала план, модалки при ветвлениях.

## Work State

### Completed

- Карта workspace: goldhub crates/дерево/размеры; GoldSrc.rs структура; список references.
- Прочитаны: `goldhub-physics/src/{lib,movement,constants,controller,player,trace}.rs`; ReGameDLL `pm_shared/pm_shared.cpp` (PM_Accelerate 998, PM_WalkMove 1039, PM_Friction 1251, PM_AirMove 1490, PM_CategorizePosition 1620, PM_CheckStuck 1711, PM_LadderMove 2228, PM_CheckFalling 2907, PM_PlayerMove 3152, PM_Jump 2635), `pm_shared.h`, `dlls/util.h`, `rehlds/engine/pmove.cpp`.
- Субагент-аудиты (готовы): formats/render, net/prediction/lagcomp, audio/footsteps, инвентарь контента CS.
- Прочитаны `goldhub-app/src/combat/weapon.rs`, `player/mod.rs` (частично), grep фич-ключевых слов.
- Ключевые findings formats: BLOCKER lightmap gamma-таблица инвертирована `bsp_transpile.rs:22-31`, `DEFAULT_LIGHTSCALE=256` hardcoded `:12`; HIGH unchecked `width*height` u32 (`bsp.rs:299,315`; `wad.rs:84,100`; `spr.rs:183,245`); MDL `pal_offset` overflow `mdl.rs:504-515`; skin-family remap игнорируется `mdl.rs:685-688`; текстура 0×0 → UV inf `bsp_transpile.rs:376-389`.
- Net: netchan-заголовок совпадает с ReHLDS, но reliable/фрагментация/CRC отсутствуют; GOLDSRC_48 не интероперабелен (OOB challenge бинарный vs текст `oob.rs:50-60,93-103`); prediction — toy-интегратор `prediction/state.rs:34-117`; ReHLDS lag comp отсутствует; баги jitter/rollback/raycast.
- Audio: материалы почти полные, но `PM_FindTextureType` prefix-match неверен `materials.rs:265-269`, неизвестный material-char рушит парс `materials.rs:197-202`, WAV loop cue offset 32 vs 24 `wav.rs:161`, CHAN_BODY правила не портированы.

### Active

- Идёт аудит `goldhub-app`: `entity/mod.rs` (1698), `studio/{animation,viewmodel,world_entity,store,mod}.rs`, `hud/mod.rs`, `level/mod.rs`, `camera`, `mesh_bridge`, `sprite`; остаток `trace.rs`.
- Нужна верификация утверждений субагентов и синтез итогового отчёта на русском (корректно / с ошибками / отсутствует, severity + file:line, соответствие bugfix-клиентам и roadmap stitch-rs).

### Blocked

- (none) — жёстких блокеров нет; goldhub вне графа, только файловое чтение.

## Next Move

1. Дочитать `goldhub-app` (entity/studio/hud/level/camera/sprite) + остаток `trace.rs`; проверить коллизии clipnodes/hull.
2. Собрать структурированный аудит-отчёт (RU): ✅ корректно, ⚠️ ошибки с severity/file:line, ❌ отсутствует; отдельно — паритет с NextClient/CS_Patches/Xash3D, риски 128 Гц и duck hull, план миграции на stitch-rs.

## Relevant Files

- `D:/Repo/goldhub/crates/goldhub-physics/src/{constants,movement,trace,controller,player}.rs`: ядро физики, основные отклонения.
- `D:/Repo/goldhub/crates/goldhub-app/src/{combat/weapon.rs,player/mod.rs,entity/mod.rs,studio/*,hud/mod.rs,level/mod.rs}`: геймплей/рендер, вертикальный срез.
- `D:/Repo/goldhub/crates/goldhub-formats/src/codecs/legacy/{bsp,mdl,spr,wad}.rs`, `transpile/bsp_transpile.rs`: парсеры/транспиляция, найдены BLOCKER/HIGH.
- `D:/Repo/goldhub/crates/goldhub-net/src/{protocol/oob.rs,prediction/state.rs,lagcomp/*}`: протокол/предсказание/lagcomp.
- `D:/Repo/goldhub/crates/goldhub-audio/src/{materials.rs,wav.rs,footsteps.rs}`.
- `D:/Repo/GoldSrc.rs/references/regamedll/regamedll/pm_shared/pm_shared.cpp` и `dlls/util.h`: эталон физики и hull-констант.
- `D:/Repo/GoldSrc.rs/references/{rehlds,cs16_goldsrc_client,xash3d-fwgs,nextclient,cs_patches}`: эталоны движка/клиента/bugfix.
- `D:/Repo/goldhub/docs/MASTER_REFERENCE_AND_BUGFIX_BLUEPRINT.md`: заявленный план vs фактический код.

---

## Аудит GoldSrc→Rust: сводка (проверено по коду и эталонам)

Коротко: `goldhub` — это **вертикальный срез на Bevy** (рендер BSP/MDL, ходьба, 7 пушек, двери/заложники/HUD), а не движок. Ядро движения — добротный порт `pm_shared`, но с фиксированным 128 Гц и рядом отклонений. Парсеры/рендер имеют блокирующий баг освещения; сеть не интероперабельна; геймплей CS почти отсутствует.

Важно: два пункта из промежуточного handoff **сняты как ошибочные** (проверил эталон):

- Duck-hull `[-16,-16,-18]/[16,16,18]` — **корректно**: это движение-халл `model->hulls[3]` / `player_mins[1]` (rehlds `pmove.cpp:33-44`). Значение z=32 из `VEC_DUCK_HULL_MAX` — это bbox игровой сущности, не PM-халл.
- `PM_FindTextureType` prefix-match — **корректно**: `name.starts_with(entry)` совпадает с `Q_strncasecmp(t->name,name,strlen(t->name))` (`materials.rs:265-269`).

---

### ✅ Перенесено корректно (verified)

- Индексы/размеры халлов 0/1/2/3 и duck=36 (`constants.rs:3-18`) ↔ `model->hulls` + `player_mins`.
- `VEC_VIEW=17` (`constants.rs:21`) ↔ `regamedll/dlls/util.h:81`.
- Air-strafe accel: cap 30 к `addspeed`, но `accelspeed=airaccel*wishspeed*dt` (`movement.rs:154-179`) ↔ `PM_AirAccelerate`.
- Прыжок 268.328 + `fuser2`-штраф + двойной `FixupGravity` (half-step до/после) — совпадает с `PM_Jump`+`PM_FixupGravityVelocity` (`movement.rs:520-544`, `pm_shared.cpp:2809-2822`). Half-step интеграция гравитации — канонная, не самоделка.
- `PM_ClipVelocity`/STOP_EPSILON, `PM_Friction` (формула), `PM_FlyMove` (мультиплейн), `PM_CategorizePosition` (вниз 2 юнита), рекурсивный hull-check — структурно совпадают (`movement.rs:70-330`, `trace.rs:138-274`).
- Collision transpile: hull 0 из `nodes`, hull 1-3 из глобального clipnode-лумпа (`collision_transpile.rs:22-69`) — верно.
- BSP/MDL/SPR/WAD парсинг, netchan-заголовок, materials/footsteps/recoil — в основном на месте.

### ⚠️ Баги/отклонения (severity, file:line)

**BLOCKER**

- Гамма-таблица лайтмапов инвертирована: `pow(i/1023, 1/lightgamma)` (`bsp_transpile.rs:22-31`) вместо `pow(i/1023, lightgamma)` → `pow(f, 1/v_gamma)` + brightness-кривая (`xash3d engine/client/gamma.c:62-74`). Плюс нет lightstyle-анимации/динамических источников; `decode_luxel` игнорирует `g_lightstylevalue` (`bsp_transpile.rs:42-46`). Итог: неверная яркость/кривая света.

**HIGH**

- `PM_Accelerate`: `max(wishspeed,stopspeed)`, без `pmove->friction` (`movement.rs:125-150`) ↔ `accel*frametime*wishspeed*pmove->friction` (`pm_shared.cpp:1025`). Разъезд на низких wishspeed.
- `PM_WalkMove` упрощён: нет `oldonground`, `basevelocity`, `PM_CheckVelocity`; иной step-алгоритм (`movement.rs:332-470` ↔ `pm_shared.cpp:1039+`).
- Фиксированные 128 Гц вместо `cmd.msec` (`constants.rs:29-32`, `controller.rs:56-66`) — сознательное отклонение от оригинального переменного шага; влияет на окна bhop/ввод.
- В `pm_player_move` отсутствуют ветки MOVETYPE (TOSS/FLY/NOCLIP/spectator) и вода (`movement.rs:1000+`).
- Ladder: нормаль угадывается по тончайшей оси AABB (`trace.rs:639-672`) вместо реальной плоскости трейса; запускается без `!dead && !(FL_ONTRAIN)`; прыжок `normal*270` (`movement.rs:868-874`).
- `PM_CheckStuck`: детерминированный полный проход таблицы каждый тик (`movement.rs:600-628`) вместо канонного `rgStuckLast` round-robin.
- MDL: skin-family remap игнорируется (`mdl.rs:685-688`); `pal_offset` overflow (`mdl.rs:504-515`).
- Непроверенные `w*h` u32 переполнения (`bsp.rs:299,315`; `wad.rs:84,100`; `spr.rs:183,245`); текстура 0×0 → UV inf (`bsp_transpile.rs:376-389`).
- Prediction — игрушечный интегратор, не `PM_Move`, но используется rollback (`net/prediction/state.rs:34-117`; `rollback.rs:34,72`).

**MEDIUM**

- `PM_Friction`: нет `edgefriction` и `pmove->friction`; при speed<0.1 не обнуляет скорость (`movement.rs:100-122`).
- Duck/unduck — кастомные сглаженные переходы (`VEC_VIEW_DUCKING=12`, `F_MORE=18`, 200 мс), нужен построчный паритет с `PM_Duck` (`movement.rs:650+`).
- Нет `PM_CheckVelocity` (NaN/clamp) — критично для устойчивости.
- Нет `PM_AddToTouched`/impact-событий, `PM_UpdateStepSound`, `PM_PlayWaterSounds`; fall-урон отсутствует (только звук, `audio/mod.rs:311-337`).
- WAV cue sample offset читается `[32..36]` вместо `[24..28]` (`wav.rs:161`).
- Materials: неизвестный material-char рушит весь парс (`materials.rs:197-202`) вместо skip.
- Net: `contains_fragments` всегда false, нет reliable-очереди/пересборки/CRC (`connection/channel.rs:99-101`), нет ReHLDS lag-comp; app сетью не пользуется.
- Составной трейс сущностей — только AABB, без studio hitbox (`trace.rs:704-838`).

**LOW**

- Step-back в hull-check уменьшает локальную `frac` на 0.1, а не глобальную (`trace.rs:254-268`).
- При brush `startsolid` не выставляется `endpos=start` (`trace.rs:479-483`).
- Out-of-range node в hull-check молча даёт `CONTENTS_EMPTY` (`trace.rs:116-125`) — против fail-fast.

### ❌ Отсутствует (крупно)

- Геймплей CS: раунды, команды, buy-зоны/меню, деньги, счёт, респавн, фриз-тайм, бомба/C4 (plant/defuse), правила спасения заложников, GameRules, урон/HP/броня/смерть игрока (grep: нет round/money/score/team/gamerules).
- 23 из 30 оружий (в `weapon.rs:20-29` только Knife/Glock/USP/Deagle/AK/M4/AWP).
- Вода/плавание/waterjump, движущиеся платформы/поезда/конвейеры (`basevelocity`), физика гранат/снарядов, C4-взрыв.
- Spectator/noclip в физике (в app есть только free-camera), полноценный netcode/lag-comp.
- Radar, killfeed, scoreboard, radio, buy-menu; FastDL, HRTF, Opus.
- Parity с NextClient/CS_Patches; **GoldClient/GSClient в `references/` отсутствуют** — паритет с ними заявлять нельзя.

### Тесты

`goldhub-physics` имеет 16 тестов в `lib.rs` + 3 в `trace.rs`, но **нет golden-векторов против канонного `pm_shared`** — то есть корректность движения не зафиксирована.

---

### Приоритетный план

1. Починить гамму лайтмапов + lightstyles (BLOCKER).
2. Добавить golden-тесты физики против ReGameDLL; решить 100 Гц `cmd.msec` vs 128 Гц.
3. `PM_CheckVelocity`, `edgefriction`, `basevelocity`, вода.
4. Заменить toy-prediction на `goldhub-physics`, подключить rollback.
5. Reliable netchan + lag-comp (если нужен мультиплеер).
6. Расширить оружие/сущности/GameRules.
7. Миграция на stitch-rs — после фикса корректности.

Что дальше: (a) начать фиксить блокеры, (b) добить полный аудит `goldhub-app`/рендера, (c) расписать план миграции на stitch-rs?

---

User:

Сможешь ли ты портировать игру так, чтобы потом можно было вынести отдельный движок, и построить игру поверх движка, т.е. не вшивать в кс сам GoldSrc.rs и при этом, чтобы мы легко могли его переписать на новую более чистую архитектуру, использовать мои проекты для оптимизации, сделать многопоток, ну и в целом оптимизировать, чтобы игра летала. Такая игра не должна идти всего-лишь в 300 фпс на моем железе, согласен? I9-11900h + RTX 3060 Laptop + 16gb ddr4? Ты же понимаешь вектор развития моей экосистемы в целом, так и проектов связанных с GoldSrc.rs?

---

Agnet:

Да, смогу — но как поэтапную ре-реализацию, а не «одним PR». И да, 300 FPS на i9-11900H + RTX 3060 Laptop для контента 1999 года — это не потолок, а симптом текущей архитектуры. Цель — 1000–2000+ FPS при 1080p (дальше упрёшься в дисплей/ввод/драйвер и троттлинг ноутбука). 16 ГБ для CS-контента более чем достаточно.

### Про 300 FPS — честно

Сейчас узкое место почти наверняка не физика (128 Гц симуляция — копейки), а фронтенд:

- BspWorldCollision::new(...) и EntityCompositeTrace пересоздаются в системах каждый кадр (player/mod.rs:353,398, weapon.rs:925, studio/viewmodel.rs:323, studio/world_entity.rs:159,482);
- solid_entities — Vec, аллоцируется на кадр (player/mod.rs:265);
- тяжёлый DefaultPlugins (PBR/UI/текст), нет батчинга/инстансинга, PVS (visdata парсится, но не используется), нет occlusion/frustum-каскадов;
- PresentMode::Fifo (lib.rs:66) и отсутствие профилирования. Физика и сеть тут ни при чём — их надо сначала отделить и замерить. Целевой бюджет: sim < 0.2 мс, render-CPU < 1 мс, GPU — по fill rate. Тогда 1000+ FPS реальны; на ноутбуке потолок может дать питание/температура.

### Архитектура: игра НЕ внутри движка

Правильный принцип — dependency inversion: игра зависит от API, движок реализует API. Никаких use bevy::*и никаких goldhub_app::* внутри игры.

```text
engine-api      (traits + plain data, no_std-friendly, ноль Bevy)
engine-core     (job system, time, VFS, ECS/SoA, asset, audio mixer, input)
engine-goldsrc  (BSP/MDL/SPR/WAD, protocol, pm_shared physics)
game-cs16       (GameRules, weapons, entities, round FSM)  -> зависит ТОЛЬКО от engine-api
frontend-bevy   (сейчас: render/audio/input адаптеры)      -> единственное место с Bevy
frontend-wgpu   (позже: свой рендер за тем же API)
server-dedicated(headless: engine-core + engine-goldsrc + game-cs16)
sdk / mods      (GoldSrc.rs SDK и моды поверх engine-api, не внутри CS)
WorldTrace уже фактически первый такой trait (physics/trace.rs:81). Дальше нужен минимальный набор: RendererApi, AudioSink, InputSource, NetTransport, AssetSource, Clock, WorldQuery. Этого достаточно, чтобы вынести движок и потом переписать его, не трогая игру.
```

### Куда ложится твоя экосистема

| Проект | Роль в движке |
| --- | --- |
| stitch / SMA | per-tick конвейер sim→physics→anim→render-extract; убрать Box\<dyn> и pointer chasing |
| stitch-data | layout/сериализация ECS, ассетов, сетевых payload'ов; zero-copy |
| stitch-wire | UDP-транспорт, framing, resync для game loop |
| wire-ring | SPSC/MPSC очереди: job system, render commands, audio, net |
| modtree | дерево модулей/плагинов, порядок систем, иерархия сущностей, hot-reload |
| machine | FSM: движение, оружие, раунд, анимации |
| statefs | cvar/config, save/rollback, кэш ассетов, детерминированное состояние |
| narrow-ptr | mmap BSP/MDL, компактные handle'ы сущностей |
| odd-int | битовые поля протокола и on-disk структур BSP/MDL |
| abi-barrier | безопасная C-ABI граница для плагинов/легаси (SDK GoldSrc.rs) |
| proc-safety | hardening клиента, guard pages, sandbox модов |

### Многопоточность

- Авторитетная симуляция — single-thread + fixed tick (детерминизм; не параллелить pm_player_move вслепую — есть player-player touch).
- Параллельно: анимация, culling/PVS, декод ассетов, микс аудио, сеть, сборка render-world.
- Очереди — wire-ring, задания — job system на bevy_tasks сейчас, позже свой.
- stitch-пайплайн даёт compile-time цепочку без динамической диспетчеризации.

### План миграции

- [] Фаза 0. Parity-замок. Golden-тесты физики против ReGameDLL (позиции/скорости на трейсах), demo playback. Без этого «оптимизировать» нельзя.
- [] Фаза 1. Граница. Создать engine-api; вынести formats/physics/net/audio из Bevy; game-cs16 — только правила/оружие/сущности; goldhub-app → frontend-bevy.
- [] Фаза 2. Убрать горячие аллокации. Кэш collision/trace per-level; solid_entities — арена/stitch-data; включить PVS, батчинг, инстансинг.
- [] Фаза 3. Субстрат. wire-ring (jobs), machine (FSM), statefs (cvars/save), narrow-ptr/odd-int (ассеты/протокол), stitch (пайплайн).
- [] Фаза 4. Рендер. Профилирование (Tracy/puffin/RenderDoc); AutoNoVsync; при необходимости frontend-wgpu за тем же API.
- [] Фаза 5. Моды/SDK. abi-barrier + modtree; proc-safety; dedicated server.

### Чего не обещаю

- «Идеальный порт за один заход»: это месяцы, но инкрементально и с работающим срезом уже сейчас.
- 1000 FPS без замеров: Bevy может оказаться потолком — тогда меняем фронтенд, а не игру.
- Lock-free физику: авторитетный тик останется детерминированным и, скорее всего, однопоточным.

Да, вектор экосистемы я понимаю: zero-cost/no_std, monomorphic SMA-пайплайны, zero-alloc, компактные layout'ы, детерминированное состояние, lossy-UDP транспорт, ABI-safety, process hardening — и GoldSrc.rs как SDK/рантайм поверх, а не внутри игры. Перед интеграцией сверю реальные API крейтов, чтобы не выдумывать.

Question:
...

Отвечаю по пунктам и фиксирую решения. Вектор экосистемы понял: IR как источник истины, ортогональное ядро, zero-cost/zero-alloc, SMA-пайплайны, компактные layout'ы, детерминированное состояние, lossy-UDP транспорт, ABI-safety. Horizon-путь «1:1 порт и всё» — это только M1, не цель.

## Решения

**Физика — часть движка?**
Да, но как **rule-модуль движка над IR**, а не в ядре и не в игре.

- `goldhub-formats-ir` — plain data, единственный источник истины (геометрия, hulls, `PlayerState`, `MoveVars`, trace-результаты).
- Кодеки (`goldhub-formats-bsp/wad/mdl/spr/...`) — адаптеры IR.
- `goldhub-engine-physics` — адаптер правил (`pm_shared`), реализует трейты `engine-api` над IR.
- Игра не линкует физику напрямую — только `engine-api`. Это и есть точка будущей замены движка/физики.

**SMA сразу?**
Нет — **проектируем с учётом SMA, но v1 пишем обычным Rust + Bevy**. SMA фиксирует форму пайплайна; пока геймплей не валидирован, это дорого. Инварианты сейчас: без `Box<dyn>` в тике, явные стадии, мономорфизм где бесплатен. `stitch`/SMA внедряем после parity за тем же API. `wire-ring`/`stitch-data` можно брать раньше, если интеграция тривиальна.

**`goldhub-game-cstrike` сразу?**
Да, сразу, но тонким слоем. **Движок не встраиваем в игру** — это необратимо. `goldhub-app` остаётся демо-фронтендом; игра зависит только от `engine-api` + `formats-ir`; движок об игре не знает.

**wry?**
Да, как опциональный `UiBackend` для меню/консоли/настроек, **не для 3D-кадра**. Нативный HUD (Bevy/egui) остаётся. WebView грузится по требованию и выгружается; legacy UI меняется за трейтом.

**CQS Bevy vs SMA?**
Не конфликтуют: CQS/ECS — доступ к данным и планирование систем; SMA — композиция вызовов. SMA — в детерминированном ядре, Bevy — во фронтенде. Не смешивать: ECS-запросы не внутри SMA-стадий.

**Оптимизации позже?**
Разделяем:

- **Сейчас (архитектурные инварианты, retrofit дорог):** IR-as-data, zero-alloc в горячем пути, mmap ассетов, кэш коллизий уровня, детерминированный тик/снапшоты, никаких backend-типов в игре.
- **Позже (железо):** SIMD, выравнивание, арены, lock-free, GPU-driven render, meshlets — после parity и замеров.

**Horizon / half-way**
Согласен. Две вехи: **M1 Parity** (поведение CS 1.6 на чистой слоистой архитектуре) и **M2 Modern runtime** (SMA/jobs/GPU-driven/rollback, целевые FPS/RAM), при этом поведение M1 защищено golden-тестами. IR+API — то, что позволяет M2 без переписывания игры.

**Память**
Согласен с тезисом: «незанятая» = не отданная throughput, а не «можно течь». Вводим бюджеты и CI-замеры: idle без аллокаций, ассеты mmap/стриминг, wry только по требованию. Ориентир (уточним замерами): idle < ~50 МБ, de_dust2 < ~150 МБ, sim < 0.2 мс, render-CPU < 1 мс.

## Архитектура (v1)

```
goldhub-formats-ir          # plain data (истина)
   ↑            ↑
goldhub-engine-api      goldhub-formats-bsp/wad/mdl/spr/tga/wav/materials/demo
   ↑
goldhub-engine-core     (jobs, time, VFS, asset, audio mix, ECS/SoA)
goldhub-engine-physics  (правила pm_shared над IR)
goldhub-engine          (фасад: core + physics + форматы; игры не знает)
   ↑
goldhub-game-cstrike    (правила/оружие/сущности; только engine-api + IR)
   ↑
goldhub-frontend-bevy / goldhub-frontend-wry / goldhub-server / goldhub-sdk
```

Правило зависимости: **игра → API, движок → IR**. Обратных импортов нет.

## Первый инкремент (вариант 1)

- [ ] `goldhub-formats-ir`: вынести `goldhub-formats/src/ir/**` как есть; `goldhub-formats` пока реэкспортит (без ломки).
- [ ] `goldhub-engine-api`: `WorldTrace` + минимальные `MovementRules`, `Clock`, `AssetSource`; зависит от IR, без Bevy.
- [ ] `goldhub-engine-physics`: перенести `goldhub-physics`, реализует API-трейты, ноль Bevy.
- [ ] `goldhub-game-cstrike`: перенести чистые weapon defs/recoil/entity-данные из `goldhub-app`.
- [ ] `goldhub-app`: остаётся демо-фронтендом, импортирует game-crate.
- [ ] Parity harness: `tests/parity.rs` + JSON-фикстуры (jump, strafe, duck, ladder, step); генератор фикстур на C++/ReGameDLL, старт — ручные канонические векторы.
- [ ] CI: `cargo test`, `cargo clippy`, smoke-бенч.

Сейчас **не** делаем: полный редизайн на SMA, микро-оптимизации, сплит всех format-крейтов, завершение игры. Это следующие шаги за стабильными границами.

Подтверди — начинаю с каркаса `formats-ir` + `engine-api` + переноса физики (поведение не меняется, только границы).
