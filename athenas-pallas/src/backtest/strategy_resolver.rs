//! Resolve external strategy paths into runnable strategy kinds.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Concrete strategy runtime inferred from the filesystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedStrategy {
    /// Python script run with the configured Python executable.
    Python(PathBuf),
    /// C++ strategy directory compiled by CMake before launch.
    CmakeCpp(PathBuf),
    /// Already compiled executable or script with its own shebang.
    Binary(PathBuf),
}

impl ResolvedStrategy {
    /// Short runtime label for UI and diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Python(_) => "python",
            Self::CmakeCpp(_) => "cmake-cpp",
            Self::Binary(_) => "binary",
        }
    }

    /// Filesystem path that was selected.
    pub fn path(&self) -> &Path {
        match self {
            Self::Python(path) | Self::CmakeCpp(path) | Self::Binary(path) => path,
        }
    }
}

/// Resolve a strategy name/path against the current project layout.
pub fn resolve_strategy_path(strategy: &Path) -> Result<ResolvedStrategy> {
    for candidate in strategy_candidates(strategy) {
        if let Some(resolved) = detect_strategy(&candidate) {
            return Ok(resolved);
        }
    }

    Err(Error::Invalid(format!(
        "no runnable strategy found for {}. Expected a strategy name under trading/, a directory with CMakeLists.txt, a directory with strategy.py/main.py, a .py file, or a compiled binary",
        strategy.display()
    )))
}

/// Detect strategy type without project-root fallback.
pub fn detect_strategy(path: &Path) -> Option<ResolvedStrategy> {
    if path.is_dir() {
        if path.join("CMakeLists.txt").is_file() {
            return Some(ResolvedStrategy::CmakeCpp(path.to_path_buf()));
        }
        let strategy_py = path.join("strategy.py");
        if strategy_py.is_file() {
            return Some(ResolvedStrategy::Python(strategy_py));
        }
        let main_py = path.join("main.py");
        if main_py.is_file() {
            return Some(ResolvedStrategy::Python(main_py));
        }
        return None;
    }

    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("py") {
            Some(ResolvedStrategy::Python(path.to_path_buf()))
        } else {
            Some(ResolvedStrategy::Binary(path.to_path_buf()))
        }
    } else {
        None
    }
}

fn strategy_candidates(strategy: &Path) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        let key = normalize_for_dedup(&p);
        if seen.insert(key) {
            out.push(p);
        }
    };

    push(strategy.to_path_buf());
    if let Ok(cwd) = std::env::current_dir() {
        if strategy.is_relative() {
            push(cwd.join(strategy));
        }
    }

    for root in project_roots() {
        if strategy.is_relative() {
            push(root.join(strategy));
            if is_strategy_name(strategy) {
                push(root.join("trading").join(strategy));
            }
        } else if let (Some(parent), Some(name)) = (strategy.parent(), strategy.file_name()) {
            if parent.join("trading").is_dir() {
                push(parent.join("trading").join(name));
            }
        }
        for legacy in legacy_candidates(&root, strategy) {
            push(legacy);
        }
    }

    out
}

fn project_roots() -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut roots = Vec::new();
    let mut push_root = |root: PathBuf| {
        let key = normalize_for_dedup(&root);
        if seen.insert(key) {
            roots.push(root);
        }
    };

    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors() {
            if ancestor.join("trading").is_dir() {
                push_root(ancestor.to_path_buf());
            }
        }
    }

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join("trading").is_dir() {
            push_root(ancestor.to_path_buf());
        }
    }

    roots
}

fn legacy_candidates(root: &Path, strategy: &Path) -> Vec<PathBuf> {
    let parts: Vec<String> = strategy
        .components()
        .filter_map(component_str)
        .map(str::to_string)
        .collect();
    let Some(trading_ix) = parts.iter().position(|part| part == "trading") else {
        return Vec::new();
    };

    let tail = &parts[trading_ix + 1..];
    match tail {
        [scope, name] if scope == "strategies" => {
            vec![root.join("trading").join(name)]
        }
        [scope, name, file] if scope == "strategies" => {
            vec![root.join("trading").join(name).join(file)]
        }
        [cpp, scope, name] if cpp == "cpp" && scope == "strategies" => {
            vec![root.join("trading").join(format!("{name}_cpp"))]
        }
        [cpp, scope, name, file] if cpp == "cpp" && scope == "strategies" => {
            vec![root.join("trading").join(format!("{name}_cpp")).join(file)]
        }
        _ => Vec::new(),
    }
}

fn is_strategy_name(path: &Path) -> bool {
    path.is_relative() && path.components().count() == 1
}

fn component_str(component: Component<'_>) -> Option<&str> {
    match component {
        Component::Normal(s) => s.to_str(),
        _ => None,
    }
}

fn normalize_for_dedup(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    #[test]
    fn resolves_direct_strategy_name() {
        let resolved = resolve_strategy_path(Path::new("simple_sma")).unwrap();
        assert!(matches!(resolved, ResolvedStrategy::Python(_)));
        assert!(resolved.path().ends_with("trading/simple_sma/strategy.py"));
    }

    #[test]
    fn resolves_config_normalized_strategy_name() {
        let normalized = workspace_root().join("simple_sma");
        let resolved = resolve_strategy_path(&normalized).unwrap();
        assert!(matches!(resolved, ResolvedStrategy::Python(_)));
        assert!(resolved.path().ends_with("trading/simple_sma/strategy.py"));
    }

    #[test]
    fn resolves_cmake_cpp_directory() {
        let resolved = resolve_strategy_path(Path::new("simple_sma_cpp")).unwrap();
        assert!(matches!(resolved, ResolvedStrategy::CmakeCpp(_)));
        assert!(resolved.path().ends_with("trading/simple_sma_cpp"));
    }

    #[test]
    fn resolves_legacy_python_path() {
        let resolved =
            resolve_strategy_path(Path::new("trading/strategies/simple_sma/strategy.py")).unwrap();
        assert!(matches!(resolved, ResolvedStrategy::Python(_)));
        assert!(resolved.path().ends_with("trading/simple_sma/strategy.py"));
    }

    #[test]
    fn resolves_legacy_cpp_path() {
        let resolved =
            resolve_strategy_path(Path::new("trading/cpp/strategies/simple_sma")).unwrap();
        assert!(matches!(resolved, ResolvedStrategy::CmakeCpp(_)));
        assert!(resolved.path().ends_with("trading/simple_sma_cpp"));
    }

    #[test]
    fn detects_direct_python_file() {
        let resolved =
            resolve_strategy_path(&workspace_root().join("trading/simple_sma/strategy.py"))
                .unwrap();
        assert!(matches!(resolved, ResolvedStrategy::Python(_)));
    }

    #[test]
    fn invalid_strategy_errors() {
        let err = resolve_strategy_path(Path::new("definitely_missing_strategy")).unwrap_err();
        assert!(err.to_string().contains("no runnable strategy"));
    }
}
