mod copilot;
mod github;

use std::{
    error::Error,
    fmt,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{
    Response, StatusCode,
    header::{self, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;

pub use copilot::CopilotTokenProvider;
pub use github::{DeviceCode, GithubDeviceFlow};

const EDITOR_VERSION: &str = "vscode/1.131.0";
const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
const USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug)]
pub enum AuthError {
    AuthenticationRequired(PathBuf),
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Request {
        operation: &'static str,
        source: reqwest::Error,
    },
    Http {
        operation: &'static str,
        status: StatusCode,
        detail: String,
    },
    InvalidResponse {
        operation: &'static str,
        detail: String,
    },
    InvalidToken(&'static str),
    DeviceCodeExpired,
    DeviceAuthorization {
        code: String,
        description: Option<String>,
    },
    TimedOut(&'static str),
    Clock,
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationRequired(path) => write!(
                formatter,
                "GitHub authentication required at {}. Run `ccgpt auth` first.",
                path.display()
            ),
            Self::Io { operation, source } => write!(formatter, "{operation} failed: {source}"),
            Self::Request { operation, source } => {
                write!(formatter, "{operation} failed: {source}")
            }
            Self::Http {
                operation,
                status,
                detail,
            } if detail.is_empty() => write!(formatter, "{operation} failed ({status})"),
            Self::Http {
                operation,
                status,
                detail,
            } => write!(formatter, "{operation} failed ({status}): {detail}"),
            Self::InvalidResponse { operation, detail } => {
                write!(
                    formatter,
                    "{operation} returned an invalid response: {detail}"
                )
            }
            Self::InvalidToken(kind) => write!(formatter, "{kind} token is empty or invalid"),
            Self::DeviceCodeExpired => write!(formatter, "GitHub device code expired"),
            Self::DeviceAuthorization { code, description } => match description {
                Some(description) => {
                    write!(
                        formatter,
                        "GitHub authentication failed: {code}: {description}"
                    )
                }
                None => write!(formatter, "GitHub authentication failed: {code}"),
            },
            Self::TimedOut(operation) => write!(formatter, "{operation} timed out"),
            Self::Clock => write!(formatter, "system clock is before the Unix epoch"),
        }
    }
}

impl Error for AuthError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Request { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn default_github_token_path() -> Result<PathBuf, AuthError> {
    let home = dirs::home_dir().ok_or_else(|| AuthError::InvalidResponse {
        operation: "GitHub token path resolution",
        detail: "home directory is unavailable".to_owned(),
    })?;
    Ok(home
        .join(".local")
        .join("share")
        .join("ccgpt")
        .join("github_token"))
}

pub fn load_github_token(path: &Path) -> Result<String, AuthError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(AuthError::AuthenticationRequired(path.to_owned()));
        }
        Err(source) => {
            return Err(AuthError::Io {
                operation: "reading GitHub token",
                source,
            });
        }
    };
    secure_token_file(path, &file)?;
    let mut token = String::new();
    file.read_to_string(&mut token)
        .map_err(|source| AuthError::Io {
            operation: "reading GitHub token",
            source,
        })?;
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err(AuthError::AuthenticationRequired(path.to_owned()));
    }
    Ok(token)
}

pub fn save_github_token(path: &Path, token: &str) -> Result<(), AuthError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AuthError::InvalidToken("GitHub"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| AuthError::Io {
            operation: "creating GitHub token directory",
            source,
        })?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| AuthError::Io {
        operation: "opening GitHub token",
        source,
    })?;
    secure_token_file(path, &file)?;
    file.set_len(0).map_err(|source| AuthError::Io {
        operation: "truncating GitHub token",
        source,
    })?;
    file.write_all(token.as_bytes())
        .map_err(|source| AuthError::Io {
            operation: "writing GitHub token",
            source,
        })
}

#[cfg(unix)]
fn secure_token_file(_path: &Path, file: &File) -> Result<(), AuthError> {
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|source| AuthError::Io {
            operation: "securing GitHub token",
            source,
        })
}

#[cfg(windows)]
fn secure_token_file(path: &Path, _file: &File) -> Result<(), AuthError> {
    use std::{os::windows::ffi::OsStrExt, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
            SetFileSecurityW,
        },
    };

    let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)\0".encode_utf16().collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let result = unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        ) == 0
        {
            Err(io::Error::last_os_error())
        } else {
            let result = if SetFileSecurityW(
                path.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            ) == 0
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            };
            let _ = LocalFree(descriptor);
            result
        }
    };

    result.map_err(|source| AuthError::Io {
        operation: "securing GitHub token",
        source,
    })
}

fn github_headers(token: Option<&str>) -> Result<HeaderMap, AuthError> {
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert("editor-version", HeaderValue::from_static(EDITOR_VERSION));
    headers.insert(
        "editor-plugin-version",
        HeaderValue::from_static(EDITOR_PLUGIN_VERSION),
    );
    headers.insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
    headers.insert(
        "x-github-api-version",
        HeaderValue::from_static("2025-04-01"),
    );
    headers.insert(
        "x-vscode-user-agent-library-version",
        HeaderValue::from_static("electron-fetch"),
    );
    if let Some(token) = token {
        let mut value = HeaderValue::from_str(&format!("token {token}"))
            .map_err(|_| AuthError::InvalidToken("GitHub"))?;
        value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, value);
    }
    Ok(headers)
}

async fn require_success(
    response: Response,
    operation: &'static str,
) -> Result<Response, AuthError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let detail = response
        .text()
        .await
        .unwrap_or_default()
        .chars()
        .take(500)
        .collect::<String>()
        .trim()
        .to_owned();
    Err(AuthError::Http {
        operation,
        status,
        detail,
    })
}

async fn decode<T: DeserializeOwned>(
    response: Response,
    operation: &'static str,
) -> Result<T, AuthError> {
    response
        .json()
        .await
        .map_err(|source| AuthError::InvalidResponse {
            operation,
            detail: source.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::{AuthError, load_github_token, save_github_token};

    #[test]
    fn stores_and_loads_github_token() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tokens").join("github_token");
        assert!(matches!(
            load_github_token(&path),
            Err(AuthError::AuthenticationRequired(_))
        ));

        save_github_token(&path, "  github-token\n").unwrap();

        assert_eq!(load_github_token(&path).unwrap(), "github-token");
    }

    #[cfg(unix)]
    #[test]
    fn enforces_owner_only_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("github_token");
        save_github_token(&path, "github-token").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(load_github_token(&path).unwrap(), "github-token");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
