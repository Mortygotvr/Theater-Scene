//! Theater's shield for Lua redeem scripts. They're filters and animations:
//! nothing in them needs files, programs or other code, and a redeem shared as
//! a .red package could use those to harm the PC that imports it. So
//! `sandboxed_lua` leaves them out of every redeem script's Lua (calling one
//! does nothing but log that it was blocked), and `scan` finds them in a
//! script's text so importing one can warn like a virus scanner.

use mlua::{Lua, LuaOptions, StdLib, Table, Value};

/// The os functions a redeem script keeps: clocks and dates (doom.lua seeds
/// its randomness from them).
pub const SAFE_OS: &[&str] = &["time", "clock", "date", "difftime"];

/// Libraries a redeem script doesn't get at all
const BLOCKED_LIBS: &[&str] = &["io", "debug", "ffi", "package"];
/// Functions a redeem script doesn't get
const BLOCKED_FUNCTIONS: &[&str] = &["dofile", "loadfile", "load", "loadstring", "require"];
/// What `require` still hands over: libraries the script already has
/// (filters do `local bit = require("bit")`)
const BUILTIN_MODULES: &[&str] = &["bit", "jit", "string", "table", "math", "coroutine", "os"];

/// Why `call` ("io.open", "os.execute", "load"…) is blocked, or None when a
/// redeem script may make it.
pub fn why_blocked(call: &str) -> Option<&'static str> {
    let (lib, member) = call.split_once('.').unwrap_or((call, ""));
    Some(match lib {
        "io" => "reads and writes files",
        "os" => match member {
            m if SAFE_OS.contains(&m) => return None,
            "execute" => "runs programs",
            "remove" | "rename" | "tmpname" => "deletes, renames or creates files",
            "exit" => "closes OBS",
            "getenv" => "reads the PC's environment variables, which can hold passwords and keys",
            "setlocale" => "changes how OBS reads numbers and dates",
            _ => "isn't one of os's clock functions",
        },
        "dofile" | "loadfile" => "runs code from another file",
        "load" | "loadstring" => "runs code hidden in a string",
        "require" | "package" => "loads modules",
        "debug" => "reaches into the Lua engine",
        "ffi" => "calls native code directly",
        _ => return None,
    })
}

/// A Lua state for a redeem script: the standard libraries minus files,
/// programs, modules and loading code (see why_blocked). Those names still
/// exist, as calls that do nothing and return `nil, "<call> is blocked…"` the
/// way a failed io.open does, so a script that tries one keeps running: its
/// animation still finishes and puts the sources back.
pub fn sandboxed_lua() -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::OS | StdLib::BIT | StdLib::JIT,
        LuaOptions::default(),
    )?;
    let globals = lua.globals();

    let full_os: Table = globals.get("os")?;
    let os = lua.create_table()?;
    for name in SAFE_OS {
        os.set(*name, full_os.get::<Value>(*name)?)?;
    }
    os.set_metatable(Some(blocked_members(&lua, "os")?));
    globals.set("os", os)?;

    for lib in BLOCKED_LIBS {
        let blocked = lua.create_table()?;
        blocked.set_metatable(Some(blocked_members(&lua, lib)?));
        globals.set(*lib, blocked)?;
    }
    for name in BLOCKED_FUNCTIONS {
        globals.set(*name, blocked_call(&lua, name.to_string())?)?;
    }
    // Except that require still hands over the libraries the script has
    globals.set(
        "require",
        lua.create_function(|lua, name: Value| {
            let name = match name {
                Value::String(s) => s.to_string_lossy().to_string(),
                other => format!("{:?}", other),
            };
            if BUILTIN_MODULES.contains(&name.as_str()) {
                return Ok((lua.globals().get::<Value>(name.as_str())?, None));
            }
            Ok((Value::Nil, Some(blocked(lua, &format!("require(\"{}\")", name), "require"))))
        })?,
    )?;
    Ok(lua)
}

/// Logs that `shown` was blocked (`call` says why); what the script gets as
/// the error.
fn blocked(lua: &Lua, shown: &str, call: &str) -> String {
    let why = why_blocked(call).unwrap_or("isn't allowed in redeem scripts");
    let script = lua
        .app_data_ref::<crate::custom_lua::RunningScript>()
        .map(|s| s.0.display().to_string())
        .unwrap_or_default();
    crate::obs_source::trust_log(&format!("[Trust Lua] 🛡 Blocked {} in '{}': it {}", shown, script, why));
    format!("{} is blocked in redeem scripts: it {}", shown, why)
}

/// A stand-in for `call` that logs it was blocked and returns nil, why.
fn blocked_call(lua: &Lua, call: String) -> mlua::Result<mlua::Function> {
    lua.create_function(move |lua, _args: mlua::MultiValue| Ok((Value::Nil, blocked(lua, &call, &call))))
}

/// Metatable making every missing member of `lib` a blocked call.
fn blocked_members(lua: &Lua, lib: &'static str) -> mlua::Result<Table> {
    let meta = lua.create_table()?;
    meta.set(
        "__index",
        lua.create_function(move |lua, (_lib, key): (Value, Value)| {
            let key = match key {
                Value::String(s) => s.to_string_lossy().to_string(),
                other => format!("{:?}", other),
            };
            blocked_call(lua, format!("{}.{}", lib, key))
        })?,
    )?;
    Ok(meta)
}

/// A blocked call found in a script's text.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaThreat {
    pub line: usize,
    /// What it calls: "io.open", "os.execute", "load"…
    pub call: String,
    pub why: &'static str,
}

/// The blocked calls a script's text makes (see why_blocked), in order, once
/// per call and line. Comments and strings don't count. A script can still
/// hide one (`local o = os; o.execute(…)`), so this only warns: the sandbox
/// is what stops them.
pub fn scan(source: &str) -> Vec<LuaThreat> {
    let tokens = tokenize(source);
    let kind = |i: usize| tokens.get(i).map(|t| &t.kind);
    let mut threats: Vec<LuaThreat> = Vec::new();
    for (i, tok) in tokens.iter().enumerate() {
        let Tok::Name(name) = &tok.kind else { continue };
        // A field (x.io, obj:load) or a local's own name isn't the global
        let prev = i.checked_sub(1).and_then(kind);
        if matches!(prev, Some(Tok::Sym(s)) if s == "." || s == ":")
            || matches!(prev, Some(Tok::Name(k)) if k == "local" || k == "function")
        {
            continue;
        }
        let call = if name == "os" || BLOCKED_LIBS.contains(&name.as_str()) {
            let member = match (kind(i + 1), kind(i + 2)) {
                (Some(Tok::Sym(s)), Some(Tok::Name(m))) if s == "." || s == ":" => Some(m.clone()),
                (Some(Tok::Sym(s)), Some(Tok::Str(m))) if s == "[" => Some(m.clone()),
                _ => None,
            };
            match member {
                Some(m) => format!("{}.{}", name, m),
                // What's left of os in the sandbox is its clocks
                None if name == "os" => continue,
                None => name.clone(),
            }
        } else if BLOCKED_FUNCTIONS.contains(&name.as_str()) {
            // require("bit") just hands over a library the script has
            let module = match (kind(i + 1), kind(i + 2)) {
                (Some(Tok::Str(m)), _) => Some(m),
                (Some(Tok::Sym(s)), Some(Tok::Str(m))) if s == "(" => Some(m),
                _ => None,
            };
            if name == "require" && module.is_some_and(|m| BUILTIN_MODULES.contains(&m.as_str())) {
                continue;
            }
            name.clone()
        } else {
            continue;
        };
        let Some(why) = why_blocked(&call) else { continue };
        if !threats.iter().any(|t| t.line == tok.line && t.call == call) {
            threats.push(LuaThreat { line: tok.line, call, why });
        }
    }
    threats
}

/// The string literals in a script's code (not its comments), unescaped.
pub fn string_literals(source: &str) -> Vec<String> {
    tokenize(source)
        .into_iter()
        .filter_map(|t| match t.kind {
            Tok::Str(s) => Some(s),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Str(String),
    Sym(String),
    Number,
}

struct Token {
    line: usize,
    kind: Tok,
}

/// Just enough of a Lua lexer to tell code from comments and strings.
fn tokenize(source: &str) -> Vec<Token> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            i += 2;
            if let Some(level) = long_bracket_level(&chars, i) {
                let (end, text) = read_long(&chars, i, level);
                line += text.matches('\n').count();
                i = end;
            } else {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
        } else if let Some(level) = long_bracket_level(&chars, i) {
            let (end, text) = read_long(&chars, i, level);
            out.push(Token { line, kind: Tok::Str(text.strip_prefix('\n').unwrap_or(&text).to_string()) });
            line += text.matches('\n').count();
            i = end;
        } else if c == '"' || c == '\'' {
            let start_line = line;
            let mut s = String::new();
            i += 1;
            while i < chars.len() && chars[i] != c && chars[i] != '\n' {
                if chars[i] != '\\' {
                    s.push(chars[i]);
                    i += 1;
                    continue;
                }
                i += 1;
                let Some(&e) = chars.get(i) else { break };
                i += 1;
                match e {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    '\n' => {
                        s.push('\n');
                        line += 1;
                    }
                    'z' => {
                        while i < chars.len() && chars[i].is_whitespace() {
                            if chars[i] == '\n' {
                                line += 1;
                            }
                            i += 1;
                        }
                    }
                    d if d.is_ascii_digit() => {
                        let mut code = d.to_digit(10).unwrap_or(0);
                        for _ in 0..2 {
                            match chars.get(i).and_then(|c| c.to_digit(10)) {
                                Some(v) => {
                                    code = code * 10 + v;
                                    i += 1;
                                }
                                None => break,
                            }
                        }
                        s.extend(char::from_u32(code));
                    }
                    other => s.push(other),
                }
            }
            i += 1;
            out.push(Token { line: start_line, kind: Tok::Str(s) });
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token { line, kind: Tok::Name(chars[start..i].iter().collect()) });
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            while i < chars.len() {
                let d = chars[i];
                if d.is_ascii_alphanumeric() || d == '.' {
                    i += 1;
                } else if (d == '-' || d == '+') && matches!(chars[i - 1], 'e' | 'E' | 'p' | 'P') {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Token { line, kind: Tok::Number });
        } else if c == '.' || c == ':' {
            // "..", "..." and "::" aren't field access
            let start = i;
            while i < chars.len() && chars[i] == c && i - start < 3 {
                i += 1;
            }
            out.push(Token { line, kind: Tok::Sym(chars[start..i].iter().collect()) });
        } else {
            out.push(Token { line, kind: Tok::Sym(c.to_string()) });
            i += 1;
        }
    }
    out
}

/// At `i`: `[`, any number of `=`, `[` — a long string or comment. Its level
/// (the number of `=`).
fn long_bracket_level(chars: &[char], i: usize) -> Option<usize> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let mut j = i + 1;
    while chars.get(j) == Some(&'=') {
        j += 1;
    }
    (chars.get(j) == Some(&'[')).then(|| j - i - 1)
}

/// The long string or comment opening at `i`: where it ends, and its text.
fn read_long(chars: &[char], i: usize, level: usize) -> (usize, String) {
    let start = (i + level + 2).min(chars.len());
    let mut j = start;
    while j < chars.len() {
        if chars[j] == ']'
            && (1..=level).all(|k| chars.get(j + k) == Some(&'='))
            && chars.get(j + level + 1) == Some(&']')
        {
            return (j + level + 2, chars[start..j].iter().collect());
        }
        j += 1;
    }
    (chars.len(), chars[start..].iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(source: &str) -> Vec<(usize, String)> {
        scan(source).into_iter().map(|t| (t.line, t.call)).collect()
    }

    #[test]
    fn scan_finds_blocked_calls_in_code_only() {
        let src = r#"
local f = io.open("x.txt", "w")        -- line 2
os.execute("calc")                     -- line 3
local t = os.time() + os.clock()       -- clocks are fine
os["remove"]("a")                      -- line 5
local code = load("return 1")          -- line 6
-- io.open in a comment, os.exit() too
--[[ dofile("a.lua")
     require("x") ]]
print("io.open and os.exit in a string")
local s = [==[ os.exit() ]==]
local obj = {}; obj.io = 1; obj:load()   -- fields aren't the globals
local x = 1 .. io.write("y")           -- line 13
local function loadstring() end        -- defining its own, not calling it
local r = require "socket"             -- line 15
local bit = require("bit")             -- a library it has anyway
"#;
        assert_eq!(
            calls(src),
            [
                (2, "io.open".to_string()),
                (3, "os.execute".to_string()),
                (5, "os.remove".to_string()),
                (6, "load".to_string()),
                (13, "io.write".to_string()),
                (15, "require".to_string()),
            ]
        );
        assert_eq!(scan("os.exit()")[0].why, "closes OBS");
    }

    #[test]
    fn doom_and_the_filters_have_nothing_blocked() {
        let mut scripts = vec![crate::paths::scripts_dir().join("doom.lua"), crate::paths::scripts_dir().join("snow_storm.lua")];
        if let Ok(dir) = std::fs::read_dir(crate::paths::filters_dir()) {
            scripts.extend(dir.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "lua")));
        }
        for script in scripts {
            if let Ok(src) = std::fs::read_to_string(&script) {
                assert_eq!(scan(&src), [], "{}", script.display());
            }
        }
    }

    #[test]
    fn string_literals_are_unescaped() {
        let src = "a = 'filters/swirl.lua' b = \"sounds\\\\boom.mp3\" -- 'not.me'\nc = [[x.png]]";
        assert_eq!(string_literals(src), ["filters/swirl.lua", "sounds\\boom.mp3", "x.png"]);
    }

    #[test]
    fn the_sandbox_blocks_without_stopping_the_script() {
        let lua = sandboxed_lua().unwrap();
        let (f, err): (Value, String) = lua.load(r#"return io.open("C:/x.txt", "w")"#).eval().unwrap();
        assert!(f.is_nil());
        assert!(err.contains("io.open is blocked"), "{}", err);
        for code in [
            "return os.execute('calc')",
            "return os.remove('x')",
            "return os.exit(0)",
            "return load('return 1')",
            "return dofile('x.lua')",
            "return require('socket')",
            "return package.loadlib('x', 'y')",
            "return debug.getregistry()",
        ] {
            let first: Value = lua.load(code).eval().unwrap_or_else(|e| panic!("{}: {}", code, e));
            assert!(first.is_nil(), "{} returned {:?}", code, first);
        }
        // What animations use still works
        let ok: bool = lua
            .load("return os.time() > 0 and os.clock() >= 0 and math.floor(2.5) == 2 and string.format('%d', 3) == '3' and #table.concat({'a','b'}) == 2 and bit.band(3, 1) == 1")
            .eval()
            .unwrap();
        assert!(ok);
        let has_bit: bool = lua.load("return require('bit') == bit and require('socket') == nil").eval().unwrap();
        assert!(has_bit);
        // The script carries on after a blocked call
        let n: i64 = lua.load("io.open('x'); os.remove('y'); return 42").eval().unwrap();
        assert_eq!(n, 42);
    }
}
