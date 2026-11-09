use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

#[test]
fn bash_wrapper_preserves_paths_failures_and_other_command_arguments() {
    exercise(ShellKind::Bash, "bash", &["--noprofile", "--norc"]);
}

#[test]
fn zsh_wrapper_preserves_paths_failures_and_other_command_arguments() {
    exercise(ShellKind::Zsh, "zsh", &["-f"]);
}

#[test]
fn fish_wrapper_preserves_paths_failures_and_other_command_arguments() {
    exercise(ShellKind::Fish, "fish", &["--no-config"]);
}

#[test]
fn bash_wrapper_matches_exactly_with_nocasematch_and_preserves_the_option() {
    let options = ["--noprofile", "--norc", "-O", "nocasematch"];
    exercise(ShellKind::Bash, "bash", &options);
    let Some(fixture) = ShellFixture::new(ShellKind::Bash, "bash", &options) else {
        return;
    };
    let destination = fixture.root.path().join("destination");
    fs::create_dir(&destination).unwrap();
    let output = fixture.run_script(
        &destination,
        "coco cd fix/login || exit 90\nshopt -q nocasematch || exit 91",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fish_wrapper_keeps_backward_and_forward_directory_history() {
    let Some(fixture) = ShellFixture::new(ShellKind::Fish, "fish", &["--no-config"]) else {
        return;
    };
    for name in ["first", "previous", "destination"] {
        fs::create_dir(fixture.root.path().join(name)).unwrap();
    }
    let destination = fixture.root.path().join("destination");
    let output = fixture.run_script(
        &destination,
        r#"
cd "$COCO_TEST_ROOT/first"; or exit 10
cd "$COCO_TEST_ROOT/previous"; or exit 11
coco cd fix/login; or exit 12
test "$PWD" = "$COCO_TEST_DESTINATION"; or exit 13
cd -; or exit 14
test "$PWD" = "$COCO_TEST_ROOT/previous"; or exit 15
nextd; or exit 16
test "$PWD" = "$COCO_TEST_DESTINATION"; or exit 17
prevd; or exit 18
test "$PWD" = "$COCO_TEST_ROOT/previous"; or exit 19
"#,
    );
    assert!(
        output.status.success(),
        "history check failed with {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn exercise(kind: ShellKind, shell: &str, options: &[&str]) {
    let Some(fixture) = ShellFixture::new(kind, shell, options) else {
        return;
    };
    let destination = fixture
        .root
        .path()
        .join("über 'quotes' $vars ; [glob] $(touch UNEXPECTED)");
    fs::create_dir(&destination).unwrap();
    let missing_directory = fixture.root.path().join("gone");
    for case in cases() {
        fixture.run_case(&destination, case);
    }
    fixture.run_case(
        &missing_directory,
        ("coco cd fix/login", vec!["path", "fix/login"], 1, false),
    );
    assert!(!fixture.root.path().join("UNEXPECTED").exists());
}

type Case = (&'static str, Vec<&'static str>, i32, bool);

fn cases() -> Vec<Case> {
    vec![
        ("coco cd fix/login", vec!["path", "fix/login"], 0, true),
        ("coco cd", vec!["path"], 0, true),
        (
            "coco -g cd fix/login",
            vec!["-g", "path", "fix/login"],
            0,
            true,
        ),
        (
            "coco '../repo path' cd fix/login",
            vec!["../repo path", "path", "fix/login"],
            0,
            true,
        ),
        (
            "coco --no-input cd fix/login -g",
            vec!["--no-input", "path", "fix/login", "-g"],
            0,
            true,
        ),
        ("coco cd missing", vec!["path", "missing"], 42, false),
        ("coco cd cancelled", vec!["path", "cancelled"], 130, false),
        ("coco cd empty", vec!["path", "empty"], 1, false),
        ("coco cd --help", vec!["cd", "--help"], 0, false),
        (
            "coco send cd 'hello world'",
            vec!["send", "cd", "hello world"],
            0,
            false,
        ),
        ("coco repo add cd", vec!["repo", "add", "cd"], 0, false),
        (
            "coco create fix/login --send cd",
            vec!["create", "fix/login", "--send", "cd"],
            0,
            false,
        ),
        (
            "coco ../repo send cd",
            vec!["../repo", "send", "cd"],
            0,
            false,
        ),
        ("coco ls cd", vec!["ls", "cd"], 0, false),
        ("coco --help cd", vec!["--help", "cd"], 0, false),
        ("coco -- cd", vec!["--", "cd"], 0, false),
        ("coco CD status", vec!["CD", "status"], 0, false),
        (
            "coco REPO cd fix/login",
            vec!["REPO", "path", "fix/login"],
            0,
            true,
        ),
        (
            "coco LS cd fix/login",
            vec!["LS", "path", "fix/login"],
            0,
            true,
        ),
    ]
}

struct ShellFixture<'a> {
    kind: ShellKind,
    shell: &'a str,
    options: &'a [&'a str],
    root: tempfile::TempDir,
    init: PathBuf,
}

impl<'a> ShellFixture<'a> {
    fn new(kind: ShellKind, shell: &'a str, options: &'a [&'a str]) -> Option<Self> {
        if Command::new(shell).arg("--version").output().is_err() {
            eprintln!("{shell} is not installed; skipping its optional process checks");
            return None;
        }
        let root = tempfile::tempdir().unwrap();
        let init = root.path().join("init");
        fs::write(&init, script(kind)).unwrap();
        let binary = root.path().join("coco");
        fs::write(&binary, include_str!("fake-coco.sh")).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        Some(Self {
            kind,
            shell,
            options,
            root,
            init,
        })
    }

    fn run_script(&self, destination: &Path, body: &str) -> std::process::Output {
        let source = if matches!(self.kind, ShellKind::Fish) {
            "source $COCO_TEST_INIT"
        } else {
            "set -u; source \"$COCO_TEST_INIT\""
        };
        Command::new(self.shell)
            .args(self.options)
            .args(["-c", &format!("{source}\n{body}")])
            .current_dir(self.root.path())
            .env("COCO_TEST_ROOT", self.root.path())
            .env("COCO_TEST_INIT", &self.init)
            .env("COCO_TEST_ARGS", self.root.path().join("args"))
            .env("COCO_TEST_DESTINATION", destination)
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .env("XDG_DATA_HOME", self.root.path().join("data"))
            .env("PATH", search_path(self.root.path()))
            .output()
            .unwrap()
    }

    fn run_case(&self, destination: &Path, case: Case) {
        let (invocation, arguments, status, changes_directory) = case;
        let kind = self.kind;
        let shell = self.shell;
        let root = self.root.path();
        let finish = if matches!(kind, ShellKind::Fish) {
            "set -l result $status; printf 'result=%s\\ncwd=%s\\n' $result $PWD"
        } else {
            "_coco_result=$?; printf 'result=%s\\ncwd=%s\\n' \"$_coco_result\" \"$PWD\""
        };
        let output = self.run_script(destination, &format!("{invocation}\n{finish}"));
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{shell}: {invocation}: {stderr}");
        assert!(
            stdout.contains(&format!("result={status}\n")),
            "{shell}: {invocation}: {stdout} {stderr}"
        );
        let expected_cwd = if changes_directory { destination } else { root };
        assert!(
            stdout.contains(&format!("cwd={}\n", expected_cwd.display())),
            "{shell}: {invocation}: {stdout} {stderr}"
        );
        assert_eq!(
            fs::read_to_string(root.join("args"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            arguments,
            "{shell}: {invocation}"
        );
        if changes_directory {
            assert!(stdout.starts_with("result="), "cd leaked output: {stdout}");
        }
    }
}

fn search_path(root: &Path) -> std::ffi::OsString {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(PathBuf::from(root)).chain(std::env::split_paths(&path)))
        .unwrap()
}
