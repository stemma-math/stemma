//! Running Lake in a library: building it and extracting its report.

use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::library::Library;
use crate::report::Report;

/// The outcome of building a library.
pub struct Build {
    pub success: bool,
    /// Lake's output, for showing the errors of a failed build.
    pub output: String,
}

/// Builds the library with `lake build`.
pub fn build(library: &Library) -> Result<Build> {
    let output = Command::new("lake")
        .arg("build")
        .current_dir(&library.dir)
        .stdin(Stdio::null())
        .output()
        .context("running `lake build`; is Lean (elan) installed?")?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(Build {
        success: output.status.success(),
        output: text,
    })
}

/// The lines of Lake's output worth showing: errors, warnings and what follows them.
pub fn problems(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.starts_with('✔') && !line.starts_with("info:") && !line.is_empty())
        .filter(|line| !line.starts_with("trace:") && !line.starts_with("Build completed"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extracts the report of a built library with `stemma-extract`.
pub fn extract(library: &Library) -> Result<Report> {
    let output = Command::new("lake")
        .args(["exe", "stemma-extract", library.name()])
        .current_dir(&library.dir)
        .stdin(Stdio::null())
        .output()
        .context("running `lake exe stemma-extract`")?;
    if !output.status.success() {
        bail!(
            "stemma-extract failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Lake may print build progress before the report, which is the last line.
    let json = stdout.lines().last().unwrap_or_default();
    serde_json::from_str(json).context("reading the report of stemma-extract")
}
