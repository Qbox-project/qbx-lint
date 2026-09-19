use qbx_lua_syntax::ast::*;
use qbx_lua_syntax::{parse, NumberValue};

fn parse_ok(source: &str) -> Chunk {
    let chunk = parse(source);
    assert!(chunk.errors.is_empty(), "unexpected errors for {source:?}: {:?}", chunk.errors);
    chunk
}

fn first_error(source: &str) -> String {
    let chunk = parse(source);
    chunk.errors.first().unwrap_or_else(|| panic!("expected an error for {source:?}")).message.clone()
}

#[test]
fn standard_statements() {
    parse_ok(
        r#"
        local a <const>, b <close> = 1, nil
        local function f(x, ...) return x, ... end
        function M.sub:method() return self end
        for i = 1, 10, 2 do goto continue ::continue:: end
        for k, v in pairs(t) do break end
        while true do end
        repeat local x = 1 until x
        if a then elseif b then else end
        do end
        ;;
        return f 'str', f { 1 }, f [[long]]
        "#,
    );
}

#[test]
fn operators_follow_lua_precedence() {
    let chunk = parse_ok("return 1 + 2 * 3 ^ 2 ^ 2 .. 'a' .. 'b' == x or not y and -z // 2 >> 1 & 3 | 4 ~ 5");
    let StmtKind::Return(exprs) = &chunk.block.stmts[0].kind else { panic!() };
    let ExprKind::Binary { op, .. } = &exprs[0].kind else { panic!() };
    assert_eq!(*op, BinOp::Or);

    let chunk = parse_ok("return 2 ^ 3 ^ 2");
    let StmtKind::Return(exprs) = &chunk.block.stmts[0].kind else { panic!() };
    let ExprKind::Binary { rhs, .. } = &exprs[0].kind else { panic!() };
    assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Pow, .. }));

    let chunk = parse_ok("return -x ^ 2");
    let StmtKind::Return(exprs) = &chunk.block.stmts[0].kind else { panic!() };
    assert!(matches!(exprs[0].kind, ExprKind::Unary { op: UnOp::Neg, .. }));
}

#[test]
fn cfx_extensions() {
    let chunk = parse_ok(
        r#"
        local model = `adder`
        local name = player?.job?.name
        local first = list?[1]
        count += 1
        text ..= 'x'
        flags |= 0x4
        flags <<= 2
        local x, y, z in coords
        local set = { .police, .ambulance }
        /* c style
           comment */
        if a != b then end
        defer print('bye') end
        local defer = 1
        defer = defer + 1
        "#,
    );
    assert_eq!(chunk.comments.len(), 1);
    let StmtKind::Local { exprs, .. } = &chunk.block.stmts[0].kind else { panic!() };
    assert!(matches!(&exprs[0].kind, ExprKind::JenkinsHash(h) if h == "adder"));
    assert!(matches!(chunk.block.stmts[3].kind, StmtKind::CompoundAssign { op: BinOp::Add, .. }));
    assert!(matches!(chunk.block.stmts[7].kind, StmtKind::Local { in_unpack: true, .. }));
    assert!(matches!(chunk.block.stmts[10].kind, StmtKind::Defer(_)));
}

#[test]
fn numbers() {
    let cases: &[(&str, NumberValue)] = &[
        ("3", NumberValue::Int(3)),
        ("0xff", NumberValue::Int(255)),
        ("0xffffffffffffffff", NumberValue::Int(-1)),
        ("3.5", NumberValue::Float(3.5)),
        (".5", NumberValue::Float(0.5)),
        ("1e2", NumberValue::Float(100.0)),
        ("5.", NumberValue::Float(5.0)),
        ("0x.8p1", NumberValue::Float(1.0)),
        ("0xA.8", NumberValue::Float(10.5)),
        ("9223372036854775808", NumberValue::Float(9223372036854775808.0)),
    ];
    for (text, expected) in cases {
        let chunk = parse_ok(&format!("return {text}"));
        let StmtKind::Return(exprs) = &chunk.block.stmts[0].kind else { panic!() };
        let ExprKind::Number(value) = &exprs[0].kind else { panic!("{text}") };
        assert_eq!(value, expected, "{text}");
    }
    assert_eq!(first_error("return 3x"), "malformed number");
}

#[test]
fn strings() {
    let chunk = parse_ok(r#"return "a\n\x41\65\u{48}\z
        b", [==[
raw]]]==]"#);
    let StmtKind::Return(exprs) = &chunk.block.stmts[0].kind else { panic!() };
    assert_eq!(exprs[0].as_string().unwrap(), "a\nAAHb");
    assert_eq!(exprs[1].as_string().unwrap(), "raw]]");
    assert!(first_error("x = 'abc").contains("unfinished string"));
    assert!(first_error(r"x = '\q'").contains("invalid escape"));
}

#[test]
fn concat_after_number_is_not_a_malformed_number() {
    parse_ok("return 1 .. 2");
    parse_ok("return a..b");
}

#[test]
fn comments_are_collected() {
    let source = "--- doc\n--[[ long ]] local x = 1 --[==[ multi\nline ]==]\n-- tail";
    let chunk = parse_ok(source);
    let texts: Vec<&str> = chunk.comments.iter().map(|c| c.content.text(source)).collect();
    assert_eq!(texts, ["- doc", " long ", " multi\nline ", " tail"]);
}

#[test]
fn recovers_from_incomplete_member_access() {
    let chunk = parse("local a = foo.\nlocal b = 2\nbar:\nlocal c = b");
    assert!(!chunk.errors.is_empty());
    assert_eq!(chunk.block.stmts.len(), 4);
    let StmtKind::Local { exprs, .. } = &chunk.block.stmts[0].kind else { panic!() };
    assert!(matches!(&exprs[0].kind, ExprKind::Field { name, .. } if name.is_missing()));
    assert!(matches!(&chunk.block.stmts[2].kind, StmtKind::Expr(e) if matches!(&e.kind, ExprKind::MethodCall { method, .. } if method.is_missing())));
}

#[test]
fn recovers_from_garbage_and_keeps_later_statements() {
    let chunk = parse("local a = = 1\n) local b = 2\nend\nlocal c = 3");
    assert!(chunk.errors.len() >= 2);
    let locals = chunk.block.stmts.iter().filter(|s| matches!(s.kind, StmtKind::Local { .. })).count();
    assert_eq!(locals, 3);
}

#[test]
fn reports_unclosed_blocks() {
    let chunk = parse("function f()\n  if x then\n");
    assert!(chunk.errors.iter().any(|e| e.message.contains("never closed")));
    assert!(first_error("x = 1 return x y = 2").contains("last statement"));
    assert!(first_error("f() = 1").contains("cannot assign"));
    assert!(first_error("a.b").contains("not a statement"));
}

#[test]
fn deep_nesting_does_not_overflow() {
    let source = format!("return {}1{}", "(".repeat(5000), ")".repeat(5000));
    let chunk = parse(&source);
    assert!(chunk.errors.iter().any(|e| e.message.contains("nesting")));
    let source = format!("x = {}{}", "{".repeat(5000), "}".repeat(5000));
    assert!(!parse(&source).errors.is_empty());
}

#[test]
fn never_panics_on_arbitrary_prefixes() {
    let source = "local t = { [1] = `a`, b = function(...) return x?.y:z('s') end, .c }\nfor i = 1, #t do t[i] += 1 end --[[ c ]]";
    for end in 0..=source.len() {
        if source.is_char_boundary(end) {
            let _ = parse(&source[..end]);
        }
    }
}
