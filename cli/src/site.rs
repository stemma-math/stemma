//! The library's site: building it with Verso, and serving it locally.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::lake;
use crate::library::Library;
use crate::report::Report;
use crate::templates;

/// The Lake target that builds the site, which `stemma init` declares.
const SITE_TARGET: &str = r#"
# The program that builds the site; `stemma preview` writes it.
[[lean_exe]]
name = "stemma-site"
srcDir = ".stemma/site"
root = "StemmaSite"
supportInterpreter = true
"#;

/// Where the site's program and output live.
fn site_dir(library: &Library) -> PathBuf {
    library.dir.join(".stemma").join("site")
}

/// Declares the site's target in `lakefile.toml` when a library lacks it.
fn ensure_site_target(library: &Library) -> Result<()> {
    let path = library.dir.join("lakefile.toml");
    let text = std::fs::read_to_string(&path)?;
    if !text.contains("name = \"stemma-site\"") {
        std::fs::write(&path, format!("{}\n{}", text.trim_end(), SITE_TARGET))?;
    }
    Ok(())
}

/// Writes the program that assembles the site from the table of contents.
fn write_program(library: &Library, report: &Report) -> Result<()> {
    let root = std::fs::read_to_string(library.root_path())?;
    let documents: Vec<&str> = report
        .modules
        .iter()
        .filter(|m| m.kind == "document")
        .map(|m| m.name.as_str())
        .collect();
    let program = templates::render(
        "site/StemmaSite.lean",
        json!({
            "name": library.name(),
            "title": library.config.library.title,
            "root_document": root.contains("#doc ("),
            "documents": documents,
        }),
    )?;
    let dir = site_dir(library);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("StemmaSite.lean"), program)?;
    Ok(())
}

/// Builds the site, returning the directory of its pages.
pub fn build(library: &Library) -> Result<std::result::Result<PathBuf, String>> {
    let build = lake::build(library)?;
    if !build.success {
        return Ok(Err(lake::problems(&build.output)));
    }
    let report = lake::extract(library)?;
    ensure_site_target(library)?;
    write_program(library, &report)?;
    let output = Command::new("lake")
        .args(["build", "stemma-site"])
        .current_dir(&library.dir)
        .stdin(Stdio::null())
        .output()
        .context("running `lake build stemma-site`")?;
    if !output.status.success() {
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        return Ok(Err(lake::problems(&text)));
    }
    let out = site_dir(library).join("out");
    let output = Command::new("lake")
        .args(["exe", "stemma-site", "--output"])
        .arg(&out)
        .current_dir(&library.dir)
        .stdin(Stdio::null())
        .output()
        .context("running `lake exe stemma-site`")?;
    if !output.status.success() {
        bail!(
            "building the site failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(Ok(out.join("html-multi")))
}

/// `stemma preview`: builds the site and serves it locally.
pub fn preview(serve: bool, port: u16, json_output: bool) -> Result<bool> {
    let library = Library::find(Path::new("."))?;
    let spinner = crate::ui::Spinner::start("Building the site", json_output);
    let built = build(&library);
    spinner.stop();
    let pages = match built? {
        Ok(pages) => pages,
        Err(problems) => {
            crate::commands::show_build_errors(&problems);
            return Ok(false);
        }
    };
    if !serve {
        if json_output {
            println!("{}", json!({ "pages": pages }));
        } else {
            crate::ui::success(format!(
                "Built the site {}",
                crate::ui::dim(pages.display())
            ));
        }
        return Ok(true);
    }
    let (listener, port) = bind(port)?;
    let url = format!("http://127.0.0.1:{port}/");
    if json_output {
        println!("{}", json!({ "pages": pages, "url": url }));
    } else {
        crate::ui::success(format!("Serving the site at {}", crate::ui::bold(&url)));
        crate::ui::note("Ctrl-C to stop.");
    }
    for stream in listener.incoming().flatten() {
        let _ = respond(stream, &pages);
    }
    Ok(true)
}

/// Binds to `port`, or to the next free one.
pub(crate) fn bind(port: u16) -> Result<(TcpListener, u16)> {
    for candidate in port..port.saturating_add(20) {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", candidate)) {
            return Ok((listener, candidate));
        }
    }
    bail!("no free port from {port}")
}

/// The content type of a file, from its extension.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// The file a request path names, if it is inside `root`.
fn resolve(root: &Path, request: &str) -> Option<PathBuf> {
    let path = request.split(['?', '#']).next()?;
    let mut out = root.to_path_buf();
    for component in Path::new(path.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.is_dir() {
        out.push("index.html");
    }
    out.is_file().then_some(out)
}

/// Answers one HTTP request with a file of the site.
pub(crate) fn respond(mut stream: TcpStream, root: &Path) -> Result<()> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    let target = percent_decode(target);
    match resolve(root, &target) {
        Some(file) => {
            let body = std::fs::read(&file)?;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                content_type(&file),
                body.len()
            )?;
            stream.write_all(&body)?;
        }
        None => {
            let body = "Not found";
            write!(
                stream,
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
        }
    }
    Ok(())
}

/// Decodes `%XX` escapes in a request path.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_only_inside_the_site() {
        let root = std::env::temp_dir().join(format!("stemma-site-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::write(root.join("index.html"), "").unwrap();
        std::fs::write(root.join("a").join("index.html"), "").unwrap();
        assert_eq!(resolve(&root, "/"), Some(root.join("index.html")));
        assert_eq!(
            resolve(&root, "/a/?x=1"),
            Some(root.join("a").join("index.html"))
        );
        assert_eq!(resolve(&root, "/../etc/passwd"), None);
        assert_eq!(resolve(&root, "/missing.html"), None);
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("/Even%20numbers/"), "/Even numbers/");
        assert_eq!(percent_decode("/100%"), "/100%");
    }
}
