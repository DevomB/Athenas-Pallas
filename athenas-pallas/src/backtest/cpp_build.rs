//! Build a C++ strategy via CMake.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Configure and compile a strategy directory that contains `CMakeLists.txt`.
pub fn build_cpp_strategy(dir: &Path) -> Result<PathBuf> {
    let toolchain = CppToolchain::detect();
    let build_dir = dir.join(toolchain.build_dir_name());
    std::fs::create_dir_all(&build_dir).map_err(Error::Io)?;
    let mut configure = std::process::Command::new("cmake");
    configure.arg("-S").arg(dir).arg("-B").arg(&build_dir);
    toolchain.apply_configure_args(&mut configure);
    let status = configure.status().map_err(Error::Io)?;
    if !status.success() {
        return Err(Error::Invalid(toolchain.configure_error()));
    }
    let status = std::process::Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .arg("--config")
        .arg("Release")
        .status()
        .map_err(Error::Io)?;
    if !status.success() {
        return Err(Error::Invalid("cmake build failed".into()));
    }
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("strategy");
    for candidate in binary_candidates(&build_dir, name) {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(Error::Invalid(format!(
        "built binary not found in {}",
        build_dir.display()
    )))
}

enum CppToolchain {
    Default,
    Mingw,
}

impl CppToolchain {
    fn detect() -> Self {
        if cfg!(windows)
            && !command_available("cl")
            && command_available("g++")
            && command_available("mingw32-make")
        {
            Self::Mingw
        } else {
            Self::Default
        }
    }

    fn build_dir_name(&self) -> &'static str {
        match self {
            Self::Default => "build",
            Self::Mingw => "build-mingw",
        }
    }

    fn apply_configure_args(&self, configure: &mut std::process::Command) {
        if matches!(self, Self::Mingw) {
            configure
                .arg("-G")
                .arg("MinGW Makefiles")
                .arg("-DCMAKE_CXX_COMPILER=g++");
        }
    }

    fn configure_error(&self) -> String {
        match self {
            Self::Default => {
                "cmake configure failed; install a C++ compiler or set CMAKE_GENERATOR/CXX"
                    .to_string()
            }
            Self::Mingw => {
                "cmake configure failed with MinGW; verify g++ and mingw32-make are on PATH"
                    .to_string()
            }
        }
    }
}

fn command_available(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("--version")
        .output()
        .is_ok()
}

fn binary_candidates(build_dir: &Path, name: &str) -> Vec<PathBuf> {
    let exe = if cfg!(windows) { ".exe" } else { "" };
    [
        build_dir.join(format!("{name}{exe}")),
        build_dir.join("Release").join(format!("{name}{exe}")),
        build_dir.join("Debug").join(format!("{name}{exe}")),
        build_dir
            .join("RelWithDebInfo")
            .join(format!("{name}{exe}")),
        build_dir.join("MinSizeRel").join(format!("{name}{exe}")),
    ]
    .into()
}
