# Neogen

Programming/automation-игра: Godot 4.x + Rust-ядро (детерминированная симуляция) + Lua-скрипты игрока. Сеттинг — соларпанк-утопия 2480: инженер-смотритель обслуживает «Ноосферу».

**Движок:** Godot 4.7 (4.7.2.stable, установлен через scoop, бинарь `godot` на PATH).

## Версии

| Компонент | Версия | Примечание |
|---|---|---|
| Godot | 4.7.2.stable | scoop, бинарь `godot` на PATH |
| godot-rust (gdext) | `=0.5.5`, фича `api-4-7` | последняя стабильная на crates.io, явно заявляет Godot 4.7 API; требует rustc ≥ 1.94 (локально — 1.98 stable) |
| mlua | `=0.12.1`, `lua54` + `vendored` | точечный пин, смена — гейт пользователя |
| rustc | stable (≥ 1.94) | CI использует `dtolnay/rust-toolchain@stable` |

Сборка расширения: `cargo build -p neogen-gdext --release`, артефакт копируется в `godot/bin/` (не коммитится); регистрация — `godot --headless --import` из `godot/`.

**Известный баг (Godot 4.7.2 + gdext 0.5.5):** headless editor-импорт с класс-регистрирующим расширением сегфолтится на выходе процесса — ПОСЛЕ записи `.godot/extension_list.cfg`. Сам краш безвреден; чистый прогон `godot --headless --quit` загружает extension без ошибок (CI проверяет именно это). Пересмотреть при обновлении Godot/gdext.
