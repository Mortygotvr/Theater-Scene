# STRICT MANDATE: CORE LOGIC IN TRUST + OBS LUA / ASSETS

## Rule Definition
1. **Core Development in `Trust/` (`libtrust.so`)**:
   - All backend calculations, physics simulation, pixel collisions, font rasterization, inter-process communication, and audio decoding must be implemented in Rust under `Trust/`.
2. **OBS Frontend & Lua Integration**:
   - The OBS scripts (`theater.lua`, `theater_manager.lua`, `theater_editor.lua`), OBS overlays, and related OBS frontend assets are fully active and can be edited/added to connect OBS to `Trust`.
3. **Non-OBS / Legacy Python Files Are Read-Only Reference**:
   - All Python scripts (`websocket_server.py`, `main.py`, `vision.py`, `scene_tracker.py`, `obs_automation.py`, `obs_client.py`) are strictly **read-only reference and documentation**.
   - Do NOT edit or modify Python backend scripts.
