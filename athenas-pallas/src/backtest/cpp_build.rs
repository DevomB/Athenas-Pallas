//! Build a C++ strategy via CMake.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Configure and compile a strategy directory that contains `CMakeLists.txt`.
pub fn build_cpp_strategy(dir: &Path) -> Result<PathBuf> {
    let build_dir = dir.join("build");
    std::fs::create_dir_all(&build_dir).map_err(Error::Io)?;
    let status = std::process::Command::new("cmake")
        .arg("-S")
        .arg(dir)
        .arg("-B")
        .arg(&build_dir)
        .status()
        .map_err(Error::Io)?;
    if !status.success() {
        return Err(Error::Invalid("cmake configure failed".into()));
    }
    let status = std::process::Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .status()
        .map_err(Error::Io)?;
    if !status.success() {
        return Err(Error::Invalid("cmake build failed".into()));
    }
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("strategy");
    let bin = if cfg!(windows) {
        build_dir.join("Release").join(format!("{name}.exe"))
    } else {
        build_dir.join(name)
    };
    if !bin.is_file() {
        let alt = build_dir.join(name);
        if alt.is_file() {
            return Ok(alt);
        }
        return Err(Error::Invalid(format!(
            "built binary not found at {}",
            bin.display()
        )));
    }
    Ok(bin)
}
