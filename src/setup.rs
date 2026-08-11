use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn install() -> Result<Vec<PathBuf>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Home directory was not found".to_string())?;
    let data = home.join(".local").join("share").join("ccgpt");
    fs::create_dir_all(&data).map_err(io_error)?;
    if cfg!(windows) {
        install_powershell(&data)
    } else {
        install_posix(&home, &data)
    }
}

fn install_powershell(data: &Path) -> Result<Vec<PathBuf>, String> {
    let integration = data.join("shell.ps1");
    fs::write(
        &integration,
        "function global:claude {\n  & ccgpt run @args\n}\nfunction global:claude-ts {\n  & ccgpt-ts run @args\n}\n",
    )
    .map_err(io_error)?;
    let profiles = powershell_profiles()?;
    for profile in &profiles {
        append_line(profile, r#". "$HOME/.local/share/ccgpt/shell.ps1""#)?;
    }
    Ok(profiles)
}

fn powershell_profiles() -> Result<Vec<PathBuf>, String> {
    let mut profiles = Vec::new();
    for executable in ["pwsh", "powershell"] {
        let output = Command::new(executable)
            .args(["-NoProfile", "-Command", "$PROFILE.CurrentUserCurrentHost"])
            .output();
        if let Ok(output) = output
            && output.status.success()
        {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !value.is_empty() {
                let profile = PathBuf::from(value);
                if !profiles.contains(&profile) {
                    profiles.push(profile);
                }
            }
        }
    }
    if profiles.is_empty() {
        Err("PowerShell was not found".into())
    } else {
        Ok(profiles)
    }
}

fn install_posix(home: &Path, data: &Path) -> Result<Vec<PathBuf>, String> {
    let integration = data.join("shell.sh");
    fs::write(
        &integration,
        "claude() {\n  command ccgpt run \"$@\"\n}\nclaude-ts() {\n  command ccgpt-ts run \"$@\"\n}\n",
    )
    .map_err(io_error)?;
    let shell = env::var("SHELL").unwrap_or_default();
    let profile = if shell.ends_with("zsh") {
        home.join(".zshrc")
    } else {
        home.join(".bashrc")
    };
    append_line(&profile, r#". "$HOME/.local/share/ccgpt/shell.sh""#)?;
    Ok(vec![profile])
}

fn append_line(path: &Path, line: &str) -> Result<(), String> {
    let current = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(io_error(error)),
    };
    if current.lines().any(|existing| existing == line) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let separator = if current.is_empty() || current.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::write(path, format!("{current}{separator}{line}\n")).map_err(io_error)
}

fn io_error(error: io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join("profile");
        append_line(&profile, "source ccgpt").unwrap();
        append_line(&profile, "source ccgpt").unwrap();
        assert_eq!(fs::read_to_string(profile).unwrap(), "source ccgpt\n");
    }
}
