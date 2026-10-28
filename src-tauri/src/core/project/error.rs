use thiserror::Error;

/// Errors of project and project-file operations.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("invalid path '{0}': only relative paths inside the project are allowed")]
    InvalidPath(String),
    #[error("'{0}' resolves outside of the project")]
    OutsideProject(String),
    #[error("'{0}' was not found")]
    NotFound(String),
    #[error("'{0}' is not a regular file")]
    NotAFile(String),
    #[error("'{0}' is a symbolic link and cannot be written")]
    SymlinkWrite(String),
    #[error("'{path}' is larger than {limit} bytes")]
    TooLarge { path: String, limit: u64 },
    #[error("'{0}' is not valid UTF-8 text")]
    NotText(String),
    #[error("'{0}' was changed on disk since it was loaded")]
    Conflict(String),
    #[error("'{0}' is not a directory")]
    NotADirectory(String),
    #[error("invalid project configuration: {0}")]
    InvalidConfig(String),
    #[error("unknown project")]
    UnknownProject,
    #[error("the project is not trusted")]
    NotTrusted,
    #[error("i/o error on '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl ProjectError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidPath(_) => "invalid_path",
            Self::OutsideProject(_) => "outside_project",
            Self::NotFound(_) => "not_found",
            Self::NotAFile(_) => "not_a_file",
            Self::SymlinkWrite(_) => "symlink_write",
            Self::TooLarge { .. } => "too_large",
            Self::NotText(_) => "not_text",
            Self::Conflict(_) => "conflict",
            Self::NotADirectory(_) => "not_a_directory",
            Self::InvalidConfig(_) => "invalid_config",
            Self::UnknownProject => "unknown_project",
            Self::NotTrusted => "not_trusted",
            Self::Io { .. } => "io",
        }
    }

    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::Conflict(_) => Some("Reload the file or overwrite it explicitly."),
            Self::NotTrusted => Some("Trust the project to enable features that execute code."),
            Self::OutsideProject(_) => {
                Some("Symbolic links pointing outside the project are not followed.")
            }
            _ => None,
        }
    }

    /// Maps an I/O error of a confined access to a project error.
    pub(crate) fn from_io(path: &str, err: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match err.kind() {
            ErrorKind::NotFound => Self::NotFound(path.to_owned()),
            ErrorKind::NotADirectory => Self::NotADirectory(path.to_owned()),
            // cap-std reports escapes (`..`, absolute or outward symlinks) as PermissionDenied.
            ErrorKind::PermissionDenied | ErrorKind::CrossesDevices => {
                Self::OutsideProject(path.to_owned())
            }
            _ => Self::Io {
                path: path.to_owned(),
                source: err,
            },
        }
    }
}
