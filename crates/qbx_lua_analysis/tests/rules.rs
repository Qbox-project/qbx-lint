use qbx_lua_analysis::scope::resolve;
use qbx_lua_analysis::summary::summarize;
use qbx_lua_analysis::{check_file, FileConfig, FileInput, Level};
use qbx_lua_syntax::parse;

fn codes_with(source: &str, config: &FileConfig) -> Vec<&'static str> {
    let chunk = parse(source);
    let resolution = resolve(&chunk);
    let summary = summarize(&chunk, &resolution);
    let input = FileInput {
        source,
        chunk: &chunk,
        resolution: &resolution,
        summary: &summary,
        config,
        side: None,
        resource: None,
    };
    check_file(&input).into_iter().map(|d| d.code).collect()
}

fn codes(source: &str) -> Vec<&'static str> {
    codes_with(source, &FileConfig::default())
}

#[test]
fn clean_snippets_stay_clean() {
    for source in [
        "local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end\nprint(fib(10))",
        "local t = {}\nfunction t:method() return self end\nprint(t)",
        "for _, v in ipairs({ 1, 2 }) do print(v) end",
        "local ok, err = pcall(error, 'x')\nprint(ok, err)",
        "local a <close> = setmetatable({}, { __close = function() end })",
        "while true do\n    local done = coroutine.yield()\n    if done then break end\nend",
        "for i = 1, 3 do\n    if i == 2 then goto continue end\n    print(i)\n    ::continue::\nend",
        "local x = 1\nlocal function get() return x end\nx = 2\nprint(get())",
        "print(N_0xdeadbeef(1), vector3(1, 2, 3).x, json.encode({}), string.strtrim(' a '))",
        "Global = Global or {}\nfunction Global.helper() end",
        "local function mayWait() end\nwhile true do\n    mayWait()\nend",
        "repeat\n    local line = io.read()\nuntil line == nil",
    ] {
        assert_eq!(codes(source), Vec::<&str>::new(), "{source}");
    }
}

#[test]
fn syntax_errors_are_not_suppressible() {
    assert_eq!(codes("-- qbx-lint: disable\nlocal = 1"), ["syntax-error"]);
}

#[test]
fn unused_and_shadowing() {
    assert_eq!(codes("for k, v in pairs({}) do print(v) end"), ["unused-loop-variable"]);
    assert_eq!(codes("local a = 1\nlocal a = 2\nprint(a)"), ["unused-local", "redefined-local"]);
    assert_eq!(codes("do ::top:: end"), ["unused-label"]);
    assert_eq!(codes("local a = 1\nprint(a < a)"), ["self-comparison"]);

    let mut config = FileConfig::default();
    config.set("shadowed-local", Level::Warning);
    assert_eq!(codes_with("local a = 1\ndo local a = 2 print(a) end\nprint(a)", &config), ["shadowed-local"]);
}

#[test]
fn ignore_prefix_silences_unused() {
    assert_eq!(codes("local _ignored = 1\nlocal function cb(_a, _b) end\ncb()"), Vec::<&str>::new());
}

#[test]
fn runtime_names_are_protected() {
    assert_eq!(codes("function GetEntityCoords() end"), ["builtin-overwrite"]);
    assert_eq!(codes("print = nil"), ["builtin-overwrite"]);
}

#[test]
fn shadowed_runtime_names_do_not_trigger_fivem_rules() {
    let source = "local Citizen = { Wait = function() end }\nlocal function GetHashKey(s) return s end\nCitizen.Wait(0)\nprint(GetHashKey('adder'))";
    assert_eq!(codes(source), Vec::<&str>::new());
}

#[test]
fn infinite_loop_detection_handles_nested_loops_and_gotos() {
    assert_eq!(codes("while true do\n    for _ = 1, 2 do break end\nend"), ["fivem/loop-never-yields"]);
    assert_eq!(codes("while true do\n    goto continue\n    ::continue::\nend"), ["fivem/loop-never-yields"]);
    assert_eq!(codes("repeat local x = GetGameTimer() until false"), ["fivem/loop-never-yields", "unused-local"]);
    assert_eq!(codes("while true do\n    if GetGameTimer() > 5 then goto done end\nend\n::done::"), Vec::<&str>::new());
}

#[test]
fn source_tracking_respects_handlers_and_locals() {
    let nested_handler = "CreateThread(function()\n    AddEventHandler('x', function() print(source) end)\nend)";
    assert_eq!(codes(nested_handler), Vec::<&str>::new());
    let shadowed = "AddEventHandler('x', function(source)\n    Wait(0)\n    print(source)\nend)";
    assert_eq!(codes(shadowed), Vec::<&str>::new());
    let awaited = "AddEventHandler('x', function()\n    local r = MySQL.query.await('q')\n    print(source, r)\nend)";
    assert_eq!(
        codes_with(awaited, &{
            let mut config = FileConfig::default();
            config.globals.push("MySQL".into());
            config
        }),
        ["fivem/source-after-yield"]
    );
}
