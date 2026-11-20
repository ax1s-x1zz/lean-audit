//! Integration tests for `axioms` that do not need Lean.
//!
//! `--lake` is a plain command name, so a shell stub can stand in for it. That
//! keeps the process boundary, the generated program and the verdict logic under
//! test on any machine, while the real-Lean run is covered by `tests/lean.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lean-audit")
}

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    /// A repository shaped like the TEAM42 ones: a library directory, a
    /// `Challenge.lean` with a deliberate `sorry`, and an `Audit.lean` that
    /// scopes its `sorry` scan to the library root.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        std::fs::create_dir_all(root.join("Lib")).unwrap();
        std::fs::write(root.join("lean-toolchain"), "leanprover/lean4:v4.33.1\n").unwrap();
        std::fs::write(
            root.join("lakefile.toml"),
            "name = \"fixture\"\nversion = \"0.1.0\"\ndefaultTargets = [\"Lib\"]\n\n[[lean_lib]]\nname = \"Lib\"\n",
        )
        .unwrap();
        std::fs::write(root.join("Lib/Base.lean"), "theorem t : True := trivial\n").unwrap();
        std::fs::write(
            root.join("Lib/Head.lean"),
            "import Lib.Base\n\ntheorem head_a : True := trivial\ntheorem head_b (p : Prop) : p ∨ ¬p := Classical.em p\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Challenge.lean"),
            "/- the comparison statement, deliberately unproved -/\ntheorem challenge_target : True := by sorry\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Audit.lean"),
            "import Lib.Head\nimport Lean\n\nopen Lean Elab Command\n\nelab \"#fixture_audit\" : command => do\n  let std : List Name := [``propext, ``Classical.choice, ``Quot.sound]\n  let expected : List Name := []\n  let heads : List Name := [``head_a, ``head_b]\n  for h in heads do\n    let axs := (← liftCoreM <| Lean.collectAxioms h).toList\n    for a in axs do\n      unless std.contains a do\n        logError m!\"FAIL\"\n  let env ← getEnv\n  let mods := env.header.moduleNames\n  let direct := env.constants.map₁.fold (init := (#[] : Array Name)) fun acc n ci =>\n    match env.getModuleIdxFor? n with\n    | some idx =>\n      if mods[idx.toNat]!.getRoot == `Lib && ci.getUsedConstantsAsSet.contains ``sorryAx then\n        acc.push n\n      else acc\n    | none => acc\n  logInfo m!\"placeholders {direct.toList}\"\n\n#fixture_audit\n",
        )
        .unwrap();

        // A stub `lake` that answers `env lean <file>` with canned output and
        // records the arguments it was given.
        let stub = root.join("fake-lake");
        std::fs::write(
            &stub,
            "#!/bin/sh\n# $1 = env, $2 = lean, $3 = generated file\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/invocation.txt\"\nif [ \"$1\" = build ]; then exit 0; fi\ncat <<'OUT'\nLEANAUDIT_AXIOMS head_a []\nLEANAUDIT_AXIOMS head_b [propext, Classical.choice, Quot.sound]\nLEANAUDIT_SORRY [Lib.Stub.unproved]\nOUT\nexit 0\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn run(&self, extra: &[&str]) -> (serde_json::Value, i32) {
        let out = Command::new(bin())
            .arg("axioms")
            .arg(self.path())
            .args(["--lake", self.path().join("fake-lake").to_str().unwrap()])
            .args(["--format", "json"])
            .args(extra)
            .output()
            .expect("run lean-audit");
        let value: serde_json::Value =
            serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
        (value, out.status.code().unwrap_or(-1))
    }
}

#[test]
fn runs_the_generated_program_and_parses_axioms() {
    let fx = Fixture::new();
    let (v, code) = fx.run(&[]);
    let r = &v["result"];

    assert_eq!(r["run"], true);
    assert_eq!(r["mode"], "collect");
    assert_eq!(r["heads_source"], "audit_file");
    assert_eq!(r["returncode"], 0);

    let heads = r["heads"].as_array().unwrap();
    assert_eq!(heads.len(), 2);
    assert_eq!(heads[0]["name"], "head_a");
    assert_eq!(heads[0]["axioms"], serde_json::json!([]));
    assert_eq!(
        heads[1]["axioms"],
        serde_json::json!(["propext", "Classical.choice", "Quot.sound"])
    );
    assert!(r["unexpected_axioms"].as_array().unwrap().is_empty());

    // The library's own `sorry` user is found; Challenge.lean's is out of scope.
    assert_eq!(r["direct_sorry"], serde_json::json!(["Lib.Stub.unproved"]));
    assert_eq!(r["sorry_scope"], serde_json::json!(["Lib"]));
    assert_eq!(r["sorry_scope_source"], "audit_file");

    // The audit publishes an expectation of zero, so this is a mismatch.
    assert_eq!(r["status"], "mismatch");
    assert_eq!(code, 3);
}

#[test]
fn imports_come_from_the_audit_file_and_lean_is_added_once() {
    let fx = Fixture::new();
    let (v, _) = fx.run(&[]);
    let imports = v["result"]["imports"].as_array().unwrap();
    assert!(imports.iter().any(|i| i == "Lib.Head"));
    assert!(imports.iter().any(|i| i == "Lean"));
    assert_eq!(
        imports.iter().filter(|i| *i == "Lean").count(),
        1,
        "Lean must not be imported twice"
    );
}

#[test]
fn the_generated_program_is_written_and_passed_to_lake() {
    let fx = Fixture::new();
    let generated = fx.path().join("generated.lean");
    let (v, _) = fx.run(&["--keep-generated", generated.to_str().unwrap()]);

    assert!(generated.is_file(), "--keep-generated must keep the file");
    let program = std::fs::read_to_string(&generated).unwrap();
    assert!(program.contains("import Lib.Head"));
    assert!(program.contains("`head_a, `head_b"));
    assert!(program.contains("Lean.collectAxioms h"));
    assert!(program.contains("`Lib"));

    // lake received: env lean <generated>
    let invocation = std::fs::read_to_string(fx.path().join("invocation.txt")).unwrap();
    let args: Vec<&str> = invocation.lines().collect();
    assert_eq!(args[0], "env");
    assert_eq!(args[1], "lean");
    assert_eq!(args[2], generated.to_str().unwrap());
    assert_eq!(v["result"]["generated_file"], generated.to_str().unwrap());
}

#[test]
fn without_keep_generated_the_temp_file_is_removed() {
    let fx = Fixture::new();
    let (v, _) = fx.run(&[]);
    let file = v["result"]["generated_file"].as_str().unwrap();
    assert!(
        !PathBuf::from(file).exists(),
        "a temp generated file must be cleaned up"
    );
}

#[test]
fn build_is_invoked_when_requested() {
    let fx = Fixture::new();
    let (v, _) = fx.run(&["--build"]);
    let b = &v["result"]["build"];
    assert_eq!(b["ran"], true);
    assert_eq!(b["returncode"], 0);
}

#[test]
fn explicit_heads_override_the_audit_file() {
    let fx = Fixture::new();
    let (v, _) = fx.run(&["--heads", "head_b"]);
    let r = &v["result"];
    assert_eq!(r["heads_source"], "cli");
    assert_eq!(r["heads"].as_array().unwrap().len(), 1);
    assert_eq!(r["heads"][0]["name"], "head_b");
}

#[test]
fn a_failing_lake_is_reported_as_error_not_as_ok() {
    let fx = Fixture::new();
    let failing = fx.path().join("failing-lake");
    std::fs::write(
        &failing,
        "#!/bin/sh\necho 'error: unknown module prefix' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&failing, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let out = Command::new(bin())
        .arg("axioms")
        .arg(fx.path())
        .args(["--lake", failing.to_str().unwrap()])
        .args(["--format", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["result"]["status"], "error");
    assert!(
        v["result"]["stderr_tail"]
            .as_str()
            .unwrap()
            .contains("unknown module prefix")
    );
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn a_missing_lake_binary_is_reported_as_error() {
    let fx = Fixture::new();
    let out = Command::new(bin())
        .arg("axioms")
        .arg(fx.path())
        .args(["--lake", "definitely-not-a-real-lake-xyz"])
        .args(["--format", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["result"]["status"], "error");
    assert_eq!(v["result"]["run"], false);
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn a_repository_without_heads_is_unsupported_not_ok() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("lean-toolchain"),
        "leanprover/lean4:v4.33.1\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("A.lean"), "theorem t : True := trivial\n").unwrap();

    let out = Command::new(bin())
        .arg("axioms")
        .arg(dir.path())
        .args(["--format", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["result"]["status"], "unsupported");
    assert_eq!(v["result"]["run"], false);
    assert_eq!(out.status.code(), Some(4));
}
