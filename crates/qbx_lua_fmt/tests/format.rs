use qbx_lua_fmt::{format, FormatError, FormatOptions, QuoteStyle};

fn fmt(source: &str) -> String {
    format(source, &FormatOptions::default()).unwrap_or_else(|e| panic!("{e}\n{source}"))
}

#[test]
fn normalises_spacing_and_indentation() {
    let source = "local   a,b=1 ,2\nif a==b   then\nprint( a ,b )\n   elseif a then return   end\nfor i=1,10 do\nfoo( i )\nend\n";
    let expected = "local a, b = 1, 2\nif a == b then\n    print(a, b)\nelseif a then\n    return\nend\nfor i = 1, 10 do\n    foo(i)\nend\n";
    assert_eq!(fmt(source), expected);
}

#[test]
fn keeps_comments_blank_lines_and_trailing_comments() {
    let source = "-- header\n\n\n\nlocal a = 1 -- trailing\n--[[ block\n   comment ]]\nlocal t = {\n  -- first\n  x = 1, -- one\n\n  y = 2,\n  -- last\n}\nif a then -- why\n  -- only a comment\nend\n";
    let expected = "-- header\n\nlocal a = 1 -- trailing\n--[[ block\n   comment ]]\nlocal t = {\n    -- first\n    x = 1, -- one\n\n    y = 2,\n    -- last\n}\nif a then\n    -- why\n    -- only a comment\nend\n";
    assert_eq!(fmt(source), expected);
}

#[test]
fn callbacks_hug_and_long_calls_break() {
    let source = "RegisterNetEvent('a:b',function(x,y)\nprint(x,y)\nend)\nCreateThread(function() end)\n";
    assert_eq!(fmt(source), "RegisterNetEvent('a:b', function(x, y)\n    print(x, y)\nend)\nCreateThread(function() end)\n");

    let long = "local result = someFunction(firstArgumentWithLongName, secondArgumentWithLongName, thirdArgumentWithLongName, fourthArgument)\n";
    let expected = "local result = someFunction(\n    firstArgumentWithLongName,\n    secondArgumentWithLongName,\n    thirdArgumentWithLongName,\n    fourthArgument\n)\n";
    assert_eq!(fmt(long), expected);
}

#[test]
fn tables_stay_expanded_or_collapse_by_width() {
    assert_eq!(fmt("local t = {a=1,b=2,'x',[k]=v}\n"), "local t = { a = 1, b = 2, 'x', [k] = v }\n");
    assert_eq!(fmt("local t = {\na=1}\n"), "local t = {\n    a = 1\n}\n");
    assert_eq!(fmt("local t = {\na=1;\n}\n"), "local t = {\n    a = 1,\n}\n");
    assert_eq!(fmt("local t = {}\nf{}\nf'x'\n"), "local t = {}\nf {}\nf 'x'\n");
}

#[test]
fn long_conditions_break_at_logical_operators() {
    let source = "if playerIsCloseEnoughToTheMarker and playerHasTheRequiredItemInInventory and notCurrentlyBusyDoingSomethingElse or forceOverride then\nend\n";
    let out = fmt(source);
    assert!(out.starts_with("if playerIsCloseEnoughToTheMarker\n    and playerHasTheRequiredItemInInventory\n"), "{out}");
}

#[test]
fn cfx_syntax_survives() {
    let source = "local h=`adder`\nlocal n=p?.job?.name\nx+=1\ns..='a'\nlocal a,b in t\nlocal s={.a,.b}\nlocal c <const> = 1\ndefer print(1) end\ngoto done\n::done::\nlocal v = -(-x)\nlocal w = - -x\nlocal z = not not y\n";
    let expected = "local h = `adder`\nlocal n = p?.job?.name\nx += 1\ns ..= 'a'\nlocal a, b in t\nlocal s = { .a, .b }\nlocal c <const> = 1\ndefer\n    print(1)\nend\ngoto done\n::done::\nlocal v = -(-x)\nlocal w = - -x\nlocal z = not not y\n";
    assert_eq!(fmt(source), expected);
}

#[test]
fn ambiguous_call_statements_keep_their_separator() {
    let out = fmt("local a = f;\n(g or h)()\n");
    assert_eq!(out, "local a = f\n;(g or h)()\n");
}

#[test]
fn comments_in_odd_places_keep_the_statement_verbatim() {
    let source = "foo(a, -- first\n    b)\nlocal   x=1\n";
    assert_eq!(fmt(source), "foo(a, -- first\n    b)\nlocal x = 1\n");
}

#[test]
fn keeps_one_line_guards_and_inline_casts() {
    let source = "if not player then return end\nlocal f = function() return 1 end\nlocal ped = GetPlayerPed(tonumber(id) --[[@as number]])\nif a then return end -- bail\n";
    assert_eq!(fmt(source), source);
    assert_eq!(fmt("if a then\nreturn\nend\n"), "if a then\n    return\nend\n");
}

#[test]
fn quote_style_is_only_changed_when_safe() {
    let options = FormatOptions { quote_style: QuoteStyle::Single, ..FormatOptions::default() };
    let out = format(r#"local a, b, c = "x", "it's", "a\n""#, &options).unwrap();
    assert_eq!(out, "local a, b, c = 'x', \"it's\", \"a\\n\"\n");
}

#[test]
fn crlf_and_tabs() {
    let options = FormatOptions { use_tabs: true, ..FormatOptions::default() };
    assert_eq!(format("if a then\r\nb()\r\nend\r\n", &options).unwrap(), "if a then\r\n\tb()\r\nend\r\n");
}

#[test]
fn refuses_files_with_syntax_errors() {
    assert!(matches!(format("local = 1", &FormatOptions::default()), Err(FormatError::SyntaxError { line: 1, .. })));
}

#[test]
fn formatting_is_idempotent() {
    let source = "local function f(a,b)\n  if a then return {x=a,y={b,1,2}} end\n  return function(...) return select('#',...) end\nend\n";
    let once = fmt(source);
    assert_eq!(fmt(&once), once);
}
