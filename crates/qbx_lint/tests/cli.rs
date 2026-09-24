use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        let path = root.join(format!("cli-{}-{}", std::process::id(), NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn read(&self, path: &str) -> Vec<u8> {
        std::fs::read(self.0.join(path)).unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_qbx-lint")).current_dir(&self.0).args(args).output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = std::fs::canonicalize(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let path = std::fs::canonicalize(&self.0).unwrap();
        assert_eq!(path.parent(), Some(root.as_path()));
        std::fs::remove_dir_all(path).unwrap();
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn edit_commands_reject_non_utf8_without_changing_bytes() {
    let fixture = Fixture::new();
    let source = b"local text='caf\xe9'\nCitizen.Wait(0)\nprint(text)\n";
    fixture.write("main.lua", source);
    for args in [vec!["fmt", "main.lua"], vec!["fmt", "--check", "main.lua"], vec!["--fix", "main.lua"]] {
        let output = fixture.run(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("not valid UTF-8"), "{output:?}");
        assert_eq!(fixture.read("main.lua"), source, "{args:?}");
    }

    let output = fixture.run(&["--format", "json", "main.lua"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("fivem/citizen-prefix"));
    assert_eq!(fixture.read("main.lua"), source, "read-only analysis preserves the original encoding");
}

#[test]
fn edit_commands_continue_to_skip_binary_escrow_and_obfuscated_files() {
    let fixture = Fixture::new();
    let obfuscated = format!("Citizen.Wait(0)\n{}\n", "local a=function(z,z)return z end;".repeat(200));
    for source in [b"FXAP\xffCitizen.Wait(0)".as_slice(), b"\x1bLua\xff", b"binary\0\xff", obfuscated.as_bytes()] {
        fixture.write("encrypted.lua", source);
        for args in [vec!["fmt", "encrypted.lua"], vec!["--fix", "encrypted.lua"]] {
            assert_success(&fixture.run(&args));
            assert_eq!(fixture.read("encrypted.lua"), source);
        }
    }
}

#[test]
fn obfuscated_files_are_skipped_and_make_their_resource_opaque() {
    let fixture = Fixture::new();
    fixture.write("fxmanifest.lua", "fx_version 'cerulean'\ngame 'gta5'\nlua54 'yes'\nclient_script 'client/*.lua'\n");
    fixture.write(
        "client/protected.lua",
        format!("Citizen.Wait(0)\n{}\n", "Hidden=function(z,z)return z end;".repeat(200)),
    );
    fixture.write("client/open.lua", "Hidden(1)\n");
    fixture.write("client/data.lua", format!("local t={{{}}}\nCitizen.Wait(t[1])\n", "1, ".repeat(2000)));
    let stdout = String::from_utf8(fixture.run(&["--format", "json", "."]).stdout).unwrap();
    assert!(!stdout.contains("protected.lua") && !stdout.contains("undefined-global"), "{stdout}");
    assert!(stdout.contains("data.lua") && stdout.contains("fivem/citizen-prefix"), "{stdout}");
}

#[test]
fn valid_utf8_including_replacement_characters_remains_editable() {
    let fixture = Fixture::new();
    fixture.write("main.lua", "local text='caf�'\nCitizen.Wait(0)\nprint(text)\n");
    assert_success(&fixture.run(&["--fix", "main.lua"]));
    assert_success(&fixture.run(&["fmt", "main.lua"]));
    let source = String::from_utf8(fixture.read("main.lua")).unwrap();
    assert!(source.contains("caf�"));
    assert!(source.contains("Wait(0)"));
    assert!(!source.contains("Citizen."));
}

#[test]
fn hash_fixes_leave_standalone_calls_valid() {
    let fixture = Fixture::new();
    fixture.write("main.lua", "GetHashKey('adder')\nprint(GetHashKey('adder'))\n");
    assert_success(&fixture.run(&["--fix", "main.lua"]));
    assert_eq!(fixture.read("main.lua"), b"GetHashKey('adder')\nprint(`adder`)\n");
}

#[test]
fn automatic_fixes_never_write_unparseable_output() {
    let fixture = Fixture::new();
    let source = b"Citizen.Wait(0)\nlocal = 1\n";
    fixture.write("main.lua", source);
    let output = fixture.run(&["--fix", "main.lua"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("automatic fixes would produce invalid Lua"));
    assert_eq!(fixture.read("main.lua"), source);
}

#[test]
fn long_expression_chains_fail_cleanly_without_modifying_source() {
    let fixture = Fixture::new();
    let source = format!("local n = {}1\nprint(n)\n", "1 + ".repeat(63_999));
    fixture.write("main.lua", &source);
    for args in [vec!["--format", "compact", "main.lua"], vec!["fmt", "--check", "main.lua"]] {
        let output = fixture.run(&args);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let message = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(message.contains("nesting"), "{message}");
        assert_eq!(fixture.read("main.lua"), source.as_bytes());
    }
}

#[test]
fn relative_and_absolute_config_paths_apply_identical_exclusions_and_overrides() {
    let fixture = Fixture::new();
    fixture.write(
        "qbxlint.toml",
        "exclude = ['skip/**']\n[[overrides]]\nfiles = ['main.lua']\nrules = { 'undefined-global' = 'off' }\n",
    );
    fixture.write("main.lua", "print(CustomGlobal)\n");
    fixture.write("skip/broken.lua", "local =\n");
    let absolute = fixture.0.join("qbxlint.toml").to_string_lossy().into_owned();
    let expected = fixture.run(&["--config", &absolute, "--format", "json", "."]);
    assert_success(&expected);
    let json: serde_json::Value = serde_json::from_slice(&expected.stdout).unwrap();
    assert_eq!(json["files"].as_array().unwrap().len(), 1);
    assert_eq!(json["files"][0]["diagnostics"].as_array().unwrap().len(), 0);
    for relative in ["qbxlint.toml", "./qbxlint.toml"] {
        let actual = fixture.run(&["--config", relative, "--format", "json", "."]);
        assert_success(&actual);
        assert_eq!(actual.stdout, expected.stdout);
    }

    fixture.write("skip/broken.lua", "Citizen.Wait(0)\n");
    assert_success(&fixture.run(&["--config", "qbxlint.toml", "--fix", "."]));
    assert_eq!(fixture.read("skip/broken.lua"), b"Citizen.Wait(0)\n");
    fixture.write("skip/broken.lua", "local n=1\nprint(n)\n");
    assert_success(&fixture.run(&["fmt", "--config", "qbxlint.toml", "skip/broken.lua"]));
    assert_eq!(fixture.read("skip/broken.lua"), b"local n=1\nprint(n)\n");
}

#[test]
fn lua_ls_settings_apply_when_no_qbxlint_toml_exists() {
    let fixture = Fixture::new();
    fixture.write(
        ".luarc.json",
        "{\n\t\"runtime.version\": \"Lua 5.4\",\n\t\"diagnostics.globals\": [\"lib\"],\n\t\"workspace.ignoreDir\": [\"\\\\[standalone\\\\]\"],\n\t\"diagnostics.disable\": [\"lowercase-global\", \"duplicate-doc-class\"]\n}\n",
    );
    fixture.write("main.lua", "helper = function() return lib end\nprint(helper, Missing)\n");
    fixture.write("[standalone]/broken.lua", "local =\n");
    for args in [vec!["--format", "json", "."], vec!["--config", ".luarc.json", "--format", "json", "."]] {
        let output = fixture.run(&args);
        assert_success(&output);
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let files = json["files"].as_array().unwrap();
        assert_eq!(files.len(), 1, "{args:?}: {json}");
        let codes: Vec<&str> =
            files[0]["diagnostics"].as_array().unwrap().iter().map(|d| d["code"].as_str().unwrap()).collect();
        assert_eq!(codes, ["undefined-global"], "{args:?}: {json}");
    }
}
