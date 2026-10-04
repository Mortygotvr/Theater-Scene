# STRICT MANDATE: PERMISSION & EXPLANATION REQUIRED FOR C++ MODIFICATIONS

## Core Rule
1. **C++ is Strictly a Passive Container**:
   - The C++ module (`Trust/src/qt_dialog.cpp` / `libtrust_qt.so` / `theater-scene.so`) exists solely to provide native Qt windows, docks, and top menu hooks for OBS.
   - All rendering, business logic, scene interaction, hitboxes, and state management belong in **Lua** and **Rust (`Trust`)**.

2. **Explicit Permission Required Before Any C++ Changes**:
   - Under NO circumstances should the agent edit, modify, or compile C++ code (`Trust/src/qt_dialog.cpp` or any `.cpp`/`.h` files) without first:
     1. Providing a clear, detailed explanation to the user of exactly WHY a C++ change is strictly necessary.
     2. Asking for and obtaining explicit user permission.

3. **Default to Lua and Rust**:
   - Always implement features, UI widgets, callbacks, and renders in **Lua** and **Rust** first.
