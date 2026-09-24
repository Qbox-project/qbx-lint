use qbx_lua_analysis::crossref::CrossRefs;
use qbx_lua_analysis::locale::LocaleFile;
use qbx_lua_analysis::project::is_not_source;
use qbx_lua_analysis::scope::resolve;
use qbx_lua_analysis::summary::summarize;
use qbx_lua_analysis::{check_file, FileConfig, FileInput, Level, Side};
use qbx_lua_syntax::parse;

fn codes_with(source: &str, config: &FileConfig) -> Vec<&'static str> {
    codes_in_project(source, config, None, &[], None)
}

/// Lints `source` as a file on `side`, with `others` (side, source) providing handlers and exports
/// as the resource `other`.
fn codes_in_project(
    source: &str,
    config: &FileConfig,
    side: Option<Side>,
    others: &[(Option<Side>, &str)],
    locale: Option<&str>,
) -> Vec<&'static str> {
    let chunk = parse(source);
    let resolution = resolve(&chunk);
    let summary = summarize(&chunk, &resolution);
    let mut crossrefs = CrossRefs::default();
    crossrefs.collect(&chunk, side, None);
    for (other_side, other) in others {
        crossrefs.collect(&parse(other), *other_side, Some("other"));
    }
    let locale = locale.map(|json| LocaleFile::parse("locales/en.json".into(), json.to_string()));
    let input = FileInput {
        source,
        chunk: &chunk,
        resolution: &resolution,
        summary: &summary,
        config,
        side,
        resource: None,
        crossrefs: Some(&crossrefs),
        locale: locale.as_ref(),
        relative_path: "",
    };
    check_file(&input).into_iter().map(|d| d.code).collect()
}

fn project(source: &str, side: Side, others: &[(Option<Side>, &str)]) -> Vec<&'static str> {
    codes_in_project(source, &FileConfig::default(), Some(side), others, None)
}

#[test]
fn events_are_checked_against_their_handlers() {
    let server = (Some(Side::Server), "RegisterNetEvent('shop:buy', function(item, amount) print(item, amount) end)");
    assert_eq!(project("TriggerServerEvent('shop:buy', 'water', 2)", Side::Client, &[server]), Vec::<&str>::new());
    assert_eq!(
        project("TriggerServerEvent('shop:buy', 'water', 2, 3)", Side::Client, &[server]),
        ["fivem/event-argument-count"]
    );
    assert_eq!(
        project("TriggerServerEvent('shop:buy', 'water')", Side::Client, &[server]),
        ["fivem/event-missing-arguments"]
    );
    assert_eq!(
        project("TriggerServerEvent('shop:buy', table.unpack({ 1 }))", Side::Client, &[server]),
        Vec::<&str>::new()
    );
    assert_eq!(project("TriggerServerEvent('unknown:event', 1)", Side::Client, &[server]), Vec::<&str>::new());

    let variadic = (Some(Side::Server), "RegisterNetEvent('log', function(...) print(...) end)");
    assert_eq!(project("TriggerServerEvent('log', 1, 2, 3)", Side::Client, &[variadic]), Vec::<&str>::new());

    let client = (Some(Side::Client), "RegisterNetEvent('hud:update', function(value) print(value) end)");
    assert_eq!(project("TriggerServerEvent('hud:update', 1)", Side::Client, &[client]), ["fivem/event-wrong-side"]);
    assert_eq!(project("TriggerClientEvent('hud:update', -1, 1)", Side::Server, &[client]), Vec::<&str>::new());
    assert_eq!(
        project("TriggerClientEvent('hud:update', -1, 1, 2)", Side::Server, &[client]),
        ["fivem/event-argument-count"]
    );
}

#[test]
fn side_checks_follow_is_duplicity_version() {
    let client = |source: &str| project(source, Side::Client, &[]);
    let wrong = ["fivem/native-wrong-side"];
    let none = Vec::<&str>::new();

    assert_eq!(client("TriggerClientEvent('a', -1)"), wrong);
    assert_eq!(client("if IsDuplicityVersion() then\n    TriggerClientEvent('a', -1)\nend"), none);
    assert_eq!(
        client("if IsDuplicityVersion() then\n    print(1)\nelse\n    TriggerClientEvent('a', -1)\nend"),
        wrong,
        "the else branch is the client"
    );
    assert_eq!(client("if IsDuplicityVersion() then\n    print(PlayerPedId())\nend"), wrong, "server-only branch");
    assert_eq!(client("if not IsDuplicityVersion() then return end\nTriggerClientEvent('a', -1)"), none);
    assert_eq!(client("local isServer = IsDuplicityVersion()\nif isServer then TriggerClientEvent('a', -1) end"), none);
    assert_eq!(
        client("local isServer = IsDuplicityVersion()\nif not isServer then TriggerClientEvent('a', -1) end"),
        wrong
    );

    let with_lib = {
        let mut config = FileConfig::default();
        config.globals.push("lib".into());
        config
    };
    let source =
        "if lib.context == 'server' then\n    TriggerClientEvent('a', -1)\nelse\n    print(PlayerPedId())\nend";
    assert_eq!(codes_in_project(source, &with_lib, None, &[], None), none);

    let handler = (None, "if IsDuplicityVersion() then\n    RegisterNetEvent('sync:push', function() end)\nend");
    assert_eq!(project("TriggerClientEvent('sync:push', -1)", Side::Server, &[handler]), ["fivem/event-wrong-side"]);
}

#[test]
fn exports_are_checked_against_their_definition() {
    let other = (Some(Side::Server), "local function getPlayer(id) return id end\nexports('GetPlayer', getPlayer)");
    assert_eq!(project("print(exports.other:GetPlayer(1))", Side::Server, &[other]), Vec::<&str>::new());
    assert_eq!(
        project("print(exports.other:GetPlayer(1, 2))", Side::Server, &[other]),
        ["fivem/export-argument-count"]
    );
    assert_eq!(project("print(exports['other']:Missing())", Side::Server, &[other]), ["fivem/unknown-export"]);
    assert_eq!(project("print(exports.notIndexed:Anything(1, 2, 3))", Side::Server, &[other]), Vec::<&str>::new());
}

#[test]
fn server_handlers_must_not_trust_the_client() {
    let trusting = "RegisterNetEvent('bank:deposit', function(src, amount)\n    local player = exports.qbx_core:GetPlayer(src)\n    player.Functions.AddMoney('bank', amount)\nend)";
    assert_eq!(
        project(trusting, Side::Server, &[]),
        ["security/client-supplied-source", "security/unvalidated-event-argument"]
    );

    let checked = "RegisterNetEvent('bank:deposit', function(amount)\n    local player = exports.qbx_core:GetPlayer(source)\n    if type(amount) ~= 'number' or amount <= 0 then return end\n    player.Functions.AddMoney('bank', amount)\nend)";
    assert_eq!(project(checked, Side::Server, &[]), Vec::<&str>::new());

    let split = "RegisterNetEvent('run')\nAddEventHandler('run', function(code) load(code)() end)";
    assert_eq!(project(split, Side::Server, &[]), ["security/unvalidated-event-argument"]);

    assert_eq!(
        project(trusting, Side::Client, &[]),
        Vec::<&str>::new(),
        "client handlers receive data from the server"
    );
}

#[test]
fn sql_must_use_placeholders() {
    let config = {
        let mut config = FileConfig::default();
        config.globals.push("MySQL".into());
        config
    };
    let lint = |source: &str| codes_in_project(source, &config, Some(Side::Server), &[], None);
    assert_eq!(
        lint("local id = 1\nMySQL.query('SELECT * FROM players WHERE id = ' .. id)"),
        ["security/sql-concatenation"]
    );
    assert_eq!(
        lint("local id = 1\nMySQL.query.await(('SELECT * FROM players WHERE id = %s'):format(id))"),
        ["security/sql-concatenation"]
    );
    assert_eq!(lint("local id = 1\nMySQL.query('SELECT * FROM players WHERE id = ?', { id })"), Vec::<&str>::new());
    assert_eq!(lint("MySQL.query('SELECT * FROM ' .. 'players')"), Vec::<&str>::new());
    assert_eq!(
        lint("local where, values = 'id = ?', { 1 }\nMySQL.query('SELECT * FROM players WHERE ' .. where, values)"),
        Vec::<&str>::new()
    );
    assert_eq!(
        lint("local name = 'players'\nMySQL.query(('SHOW COLUMNS FROM `%s`'):format(name))"),
        Vec::<&str>::new()
    );
}

#[test]
fn locale_keys_must_exist() {
    let config = {
        let mut config = FileConfig::default();
        config.globals.push("locale".into());
        config
    };
    let json = r#"{ "error": { "not_online": "Offline" }, "ok": "Fine" }"#;
    let lint = |source: &str| codes_in_project(source, &config, Some(Side::Client), &[], Some(json));
    assert_eq!(lint("print(locale('error.not_online'), locale('ok'))"), Vec::<&str>::new());
    assert_eq!(lint("print(locale('error.not_onlin'))"), ["qbox/unknown-locale-key"]);
    assert_eq!(lint("local k = 'ok'\nprint(locale(k), locale('error.' .. k))"), Vec::<&str>::new());
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
        "print(glm.normalize(vector3(1, 2, 3)), glm.pi, glm.quatLookAt(glm.forward(), glm.up()))",
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

fn hash_fixes(source: &str) -> (String, usize) {
    let chunk = parse(source);
    assert!(chunk.errors.is_empty(), "invalid test input: {source}");
    let resolution = resolve(&chunk);
    let summary = summarize(&chunk, &resolution);
    let config = FileConfig::default();
    let diagnostics = check_file(&FileInput {
        source,
        chunk: &chunk,
        resolution: &resolution,
        summary: &summary,
        config: &config,
        side: None,
        resource: None,
        crossrefs: None,
        locale: None,
        relative_path: "",
    });
    let hashes: Vec<_> = diagnostics.into_iter().filter(|d| d.code == "fivem/hash-literal").collect();
    qbx_lua_analysis::apply_fixes(source, &hashes)
}

#[test]
fn hash_literals_are_not_suggested_for_statements_or_prefix_expressions() {
    for source in [
        "GetHashKey('adder')",
        "GetHashKey('adder')()",
        "GetHashKey('adder'):method()",
        "print(GetHashKey('adder').field)",
        "print(GetHashKey('adder')[1])",
        "GetHashKey('adder').field = 1",
    ] {
        assert_eq!(hash_fixes(source), (source.to_string(), 0), "{source}");
    }
}

#[test]
fn hash_literals_are_fixed_in_values_and_parenthesized_prefix_expressions() {
    for (source, expected) in [
        ("print(GetHashKey('adder'))", "print(`adder`)"),
        ("local hash = GetHashKey('adder')", "local hash = `adder`"),
        ("return GetHashKey('adder')", "return `adder`"),
        ("print({ [GetHashKey('adder')] = true })", "print({ [`adder`] = true })"),
        ("(GetHashKey('adder'))()", "(`adder`)()"),
        ("GetHashKey('adder')(GetHashKey('other'))", "GetHashKey('adder')(`other`)"),
    ] {
        let (fixed, applied) = hash_fixes(source);
        assert_eq!(applied, 1, "{source}");
        assert_eq!(fixed, expected);
        assert!(parse(&fixed).errors.is_empty(), "fix must remain valid Lua: {fixed}");
    }
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

#[test]
fn ordinary_source_with_long_lines_is_not_mistaken_for_obfuscated_code() {
    assert!(!is_not_source(b"local x = 1\nprint(x)\n"));
    assert!(!is_not_source(format!("return '{}'", "a".repeat(4000)).as_bytes()), "small files are kept");
    let data = format!("local icon = '{}'\n", "A".repeat(8000));
    let code = "print(icon)\n".repeat(200);
    assert!(!is_not_source(format!("{data}{code}").as_bytes()), "one long data line among code");
}
