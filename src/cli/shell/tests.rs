use super::*;

#[cfg(unix)]
mod processes;

#[test]
fn initialization_uses_registered_commands_and_aliases_as_stopping_points() {
    for shell in [ShellKind::Bash, ShellKind::Zsh, ShellKind::Fish] {
        let text = script(shell);
        assert!(!text.contains("@COMMANDS@"));
        assert!(!text.contains("@SETUP@"));
        for command in ["send", "create", "jump", "path", "shell", "ls", "help"] {
            assert!(text.contains(command));
        }
        assert!(!text.contains("eval "));
    }
    assert!(script(ShellKind::Zsh).contains("emulate -L zsh"));
    assert!(!script(ShellKind::Bash).contains("emulate"));
}
