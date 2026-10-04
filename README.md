# Theater Scene

An OBS Studio plugin, written in Rust, that turns stream events into on-screen effects:

- **Redeems:** throwables that hit your avatar's real outline (not its box), water, fire, lightning, and avatar reactions (bounce, drip, jolt, or any Lua shader). Custom redeems can be written in Lua.
- **Poses:** saved positions, sizes, filters and volumes for every source in a scene, played back with an animation.
- **Scene Studio:** a window to place sources, set targets, and edit poses and redeems.
- **Rhai scripts:** OBS sources and filters written as `.rhai` files, such as the chat, the alerts and the deck. They reload when you save them.
- **Docks** for scene switching and redeems.

Chat and alerts come from a **relay** on `ws://127.0.0.1:41837`. That's served by the [Theater-Reader](https://github.com/Mortygotvr/Theater-Reader) plugin, the Theater Reader app, or anything else speaking the same messages. Poses and redeems fire only from events (redeems, subs, cheers…), never from chat messages.

## Related repos

| Repo | |
|---|---|
| [Theater-Shared](https://github.com/Mortygotvr/Theater-Shared) | Needed to build: code shared with Theater Reader |
| [Theater-Reader](https://github.com/Mortygotvr/Theater-Reader) | Optional: reads Twitch, Kick and YouTube chat inside OBS |
| [Theater-Control-Panel](https://github.com/Mortygotvr/Theater-Control-Panel) | Optional: the Control Panel in an OBS dock |

## Layout

- `Trust/`: the plugin (Rust crate `trust`, deployed as `theater-scene.dll`).
- `Trust/src/qt_dialog_win.cpp`: a small header-free Qt shim (`trust_qt.dll`) for OBS menus, docks and windows. It also serves other plugins' items in the shared 🎭 Theater menu.
- `filters/`: Rhai and Lua scripts (sources, filters, reactions).
- `scripts/`: example custom redeems in Lua (doom, snow storm).
- `Trust/assets/`: icons, fonts and textures.

## Installing a release

Extract the zip into `C:\ProgramData\obs-studio\plugins`, so you get `plugins\theater-scene\bin` and `plugins\theater-scene\data`, then restart OBS (30 or newer). Its menu items are in the **🎭 Theater** menu.

## Where its files are

- **Your data** (poses, redeems, settings, and the scripts you make in the Studio) is in `%APPDATA%\obs-studio\plugin_config\theater-scene\`.
- **What ships with it** (scripts, shaders, textures, fonts) is in the plugin's `data` folder. To edit those live from a clone of this repo, set the environment variable `THEATER_SCENE_DIR` to the clone's folder before starting OBS.

## Building (Windows)

You need Rust, Visual Studio 2022 (C++ build tools) and OBS Studio 30 or newer.

1. Clone [Theater-Shared](https://github.com/Mortygotvr/Theater-Shared) next to this repo, so `../Theater-Shared` exists.
2. `cd Trust` and `cargo build --release`.
3. Once: `Trust\gen_libs.ps1` makes import libraries from your OBS install. Then `Trust\build_trust_qt.ps1` builds `trust_qt.dll`.
4. Close OBS, then run `deploy_user.ps1`. It copies the plugin, the shim, the scripts and the assets into `C:\ProgramData\obs-studio\plugins\theater-scene` and `%APPDATA%\obs-studio\plugins\theater-scene`.
5. For a release zip: `package_release.ps1` (it takes only the files git tracks).

## Status

- Windows is the main target. The Linux build (`libtrust.so`) predates the Windows port and lags behind.
- Before version 0.1.0, your data was kept next to the code in the repo folder. The first time 0.1.0 starts, it copies it into `plugin_config\theater-scene`, and the old files are left where they were.

## License

GPL-2.0-or-later (see [LICENSE](LICENSE)), like OBS Studio.
