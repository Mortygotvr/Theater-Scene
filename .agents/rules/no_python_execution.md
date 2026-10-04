# STRICT MANDATE: DO NOT RUN PYTHON CODE

## Rule Definition
1. **Never Execute Python**:
   - Under NO circumstances should the agent execute Python code, scripts, or `python` / `python3` one-liners via `run_command` or any other tool.
2. **Inspection & Data Handling**:
   - For reading, searching, or inspecting JSON, text, or source files, always use built-in tools (`view_file`, `grep_search`) or native shell utilities (`jq`, `cat`, `grep`).
   - For compiling, building, and running tests, use Rust (`cargo`).
3. **Rationale**:
   - Running ad-hoc Python scripts introduces syntax friction, unnecessary complexity, and clutter. Adhere strictly to native tools.
